//! Threaded sending, through a scheduler rather than a plain queue.
//!
//! A send takes hundreds of milliseconds, or tens of seconds against a dead
//! relay, so a thread per send would park thousands of stacks in a 32-bit
//! process. [`WORKER_COUNT`] threads serve one queue instead.
//!
//! Nothing sleeps holding a worker: a rate limit, a retry delay and a paused
//! relay all become "not before T", and a worker takes the best job eligible
//! *now*. Best means earliest deadline, which is the enqueue time plus the
//! priority's allowance — so `HIGH` goes first, and a `LOW` that waited long
//! enough still beats a fresh `NORMAL`.
//!
//! A temporary failure is requeued with a doubling delay, up to `retries`
//! times, and the callback runs once with the final outcome. An unreachable
//! relay pauses the whole account ([`crate::account::Gate::pause`]) instead,
//! so an outage costs one probe per pause, not one per queued mail.

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use lettre::Transport;

use crate::account::{AccountHandle, classify, is_retryable};
use crate::callback::CallbackInfo;
use crate::error::{EmailError, Fail};
use crate::logger::Logger;
use crate::message::{self, MessageDraft};

/// Threads serving the send queue. The transport's own session pool is the
/// real limit on parallelism, so raising this would not help.
pub const WORKER_COUNT: usize = 4;

/// Delay before the first retry, and the first pause of an unreachable relay;
/// each further one doubles it.
const RETRY_DELAY: Duration = if cfg!(test) {
    Duration::from_millis(20)
} else {
    Duration::from_secs(5)
};

/// Upper bound on how long an idle worker sleeps before looking again. Only a
/// safety net: every change to the queue wakes the workers anyway.
const IDLE_WAIT: Duration = Duration::from_secs(1);

/// Where a message stands in the queue. `EMAIL_PRIORITY_*` in the include.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Priority {
    /// Bulk: newsletters, announcements. Never allowed to fill the queue.
    Low,
    #[default]
    Normal,
    /// A player is waiting for it: a confirmation code, a password reset.
    High,
}

impl Priority {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            -1 => Some(Self::Low),
            0 => Some(Self::Normal),
            1 => Some(Self::High),
            _ => None,
        }
    }

    /// Head start in the queue. A job's deadline is its enqueue time plus
    /// this, so it is also how long a job waits before it outranks the
    /// level above.
    fn allowance(self) -> Duration {
        match self {
            Self::High => Duration::ZERO,
            Self::Normal => Duration::from_secs(60),
            Self::Low => Duration::from_secs(600),
        }
    }

    /// Queued messages at which this priority is refused. The last quarter
    /// of the queue is kept for NORMAL and HIGH, so a bulk mailing can never
    /// lock out a password reset; HIGH is never refused by the limit.
    fn limit(self, queue_limit: u32) -> u32 {
        match self {
            Self::Low => queue_limit - queue_limit / 4,
            Self::Normal => queue_limit,
            Self::High => u32::MAX,
        }
    }
}

pub enum Job {
    /// Serialise the draft and hand it to the relay.
    Send(Box<MessageDraft>),
    /// Test-only: proves a panicking job does not take the pool with it.
    #[cfg(test)]
    Panic,
    /// Already serialised on an earlier attempt: a retry resends the same
    /// bytes instead of re-reading attachment files.
    Built(Box<lettre::Message>),
    /// Open a session and drop it, to prove host, TLS and credentials work.
    Test,
}

pub struct SendJob {
    pub account_id: i32,
    pub account: AccountHandle,
    pub job: Job,
    pub priority: Priority,
    pub callback: CallbackInfo,
}

pub struct SendResult {
    pub account_id: i32,
    /// `None` on success.
    pub error: Option<Fail>,
    /// The address the failure is about, for `OnEmailError`. Empty for a test.
    pub recipient: String,
    pub callback: CallbackInfo,
}

/// A job waiting in the queue.
struct Entry {
    job: SendJob,
    recipient: String,
    /// Scheduling key: earliest first.
    deadline: Instant,
    /// Tie-break, so equal deadlines stay first-come first-served.
    seq: u64,
    /// Retry delay; the account's [`Gate`] may hold it back further.
    not_before: Instant,
    retries_left: u32,
    delay: Duration,
}

impl Entry {
    /// A connection test reports what is true *now*: it is not held back by
    /// the account's rate limit or pause, and not retried.
    fn is_test(&self) -> bool {
        matches!(self.job.job, Job::Test)
    }

    fn eligible_at(&self) -> Instant {
        if self.is_test() {
            return self.not_before;
        }
        let gate = self.job.account.gate.lock().map(|g| g.ready_at());
        self.not_before.max(gate.unwrap_or(self.not_before))
    }
}

#[derive(Default)]
struct Queue {
    entries: Vec<Entry>,
    next_seq: u64,
    closed: bool,
}

impl Queue {
    /// Removes the best job eligible at `now`, or says when to look again.
    ///
    /// A linear scan: the queue is bounded by `queue_limit` and every pick is
    /// followed by a network round trip, so a heap would buy nothing here and
    /// could not handle eligibility that moves with each account's gate.
    fn pick(&mut self, now: Instant) -> Result<Entry, Option<Instant>> {
        let mut best: Option<usize> = None;
        let mut wake: Option<Instant> = None;

        for (i, entry) in self.entries.iter().enumerate() {
            let at = entry.eligible_at();
            if at > now {
                wake = Some(wake.map_or(at, |w| w.min(at)));
                continue;
            }
            let better = best.is_none_or(|b| {
                let current = &self.entries[b];
                (entry.deadline, entry.seq) < (current.deadline, current.seq)
            });
            if better {
                best = Some(i);
            }
        }

        let Some(i) = best else {
            return Err(wake);
        };
        let entry = self.entries.swap_remove(i);
        if !entry.is_test()
            && let Ok(mut gate) = entry.job.account.gate.lock()
        {
            gate.take(now);
        }
        Ok(entry)
    }
}

type Shared = Arc<(Mutex<Queue>, Condvar)>;

pub struct SendManager {
    queue: Shared,
    results: mpsc::Receiver<SendResult>,
    result_tx: mpsc::Sender<SendResult>,
    /// Spawned lazily: a gamemode that never sends a mail should not pay for
    /// four parked threads.
    started: bool,
    /// Across every account, including closed ones — what shutdown waits on.
    in_flight: Arc<AtomicU64>,
}

impl SendManager {
    pub fn new() -> Self {
        let (result_tx, results) = mpsc::channel();
        Self {
            queue: Arc::new((Mutex::new(Queue::default()), Condvar::new())),
            results,
            result_tx,
            started: false,
            in_flight: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Queued plus in-progress jobs, across every account.
    pub fn pending_count(&self) -> u64 {
        self.in_flight.load(Ordering::Relaxed)
    }

    /// Whether the account may queue one more job at this priority.
    pub fn check_room(account: &AccountHandle, priority: Priority) -> Result<(), Fail> {
        let limit = priority.limit(account.queue_limit);
        if account.stats.queued.load(Ordering::Relaxed) >= limit {
            return Err(EmailError::QueueFull.because(format!(
                "{limit} messages are already waiting on this account \
                 (queue_limit; LOW may use three quarters of it)"
            )));
        }
        Ok(())
    }

    /// Queues a job, or refuses it when its account is full at its priority.
    pub fn submit(&mut self, job: SendJob) -> Result<(), Fail> {
        Self::check_room(&job.account, job.priority)?;
        self.ensure_workers();

        let now = Instant::now();
        let recipient = match &job.job {
            Job::Send(draft) => draft.primary_recipient(),
            _ => String::new(),
        };
        let retries_left = if matches!(job.job, Job::Test) {
            0
        } else {
            job.account.retries
        };

        let (lock, wake) = &*self.queue;
        let Ok(mut queue) = lock.lock() else {
            return Err(EmailError::SendFailed.because("the send queue is unusable"));
        };
        if queue.closed {
            return Err(EmailError::SendFailed.because("the plugin is shutting down"));
        }

        job.account.stats.queued.fetch_add(1, Ordering::Relaxed);
        self.in_flight.fetch_add(1, Ordering::Relaxed);

        let seq = queue.next_seq;
        queue.next_seq += 1;
        queue.entries.push(Entry {
            deadline: now + job.priority.allowance(),
            job,
            recipient,
            seq,
            not_before: now,
            retries_left,
            delay: RETRY_DELAY,
        });
        wake.notify_one();
        Ok(())
    }

    /// Drains everything the workers finished since the last tick.
    pub fn poll_results(&mut self) -> Vec<SendResult> {
        self.results.try_iter().collect()
    }

    /// Waits up to `limit` for the queue to empty, and returns what is left.
    ///
    /// For shutdown: mail queued in the last seconds before an exit usually
    /// leaves within them, and there is no on-disk queue to rescue it later.
    pub fn drain(&self, limit: Duration) -> u64 {
        let deadline = Instant::now() + limit;
        while self.pending_count() > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        self.pending_count()
    }

    fn ensure_workers(&mut self) {
        if self.started {
            return;
        }
        self.started = true;

        for _ in 0..WORKER_COUNT {
            let queue = self.queue.clone();
            let results = self.result_tx.clone();
            let in_flight = self.in_flight.clone();
            thread::spawn(move || {
                while let Some(entry) = next_entry(&queue) {
                    // What the result needs if the attempt panics and takes
                    // the entry with it.
                    let failed = SendResult {
                        account_id: entry.job.account_id,
                        error: None,
                        recipient: entry.recipient.clone(),
                        callback: entry.job.callback.clone(),
                    };
                    let account = entry.job.account.clone();

                    // A panic in one send must not retire a worker: four of
                    // those and the plugin would go quiet with a full queue
                    // and no explanation.
                    let outcome =
                        std::panic::catch_unwind(AssertUnwindSafe(|| attempt(entry, &queue)));

                    let result = match outcome {
                        Ok(Some(result)) => result,
                        // Requeued for a retry: it is still in flight.
                        Ok(None) => continue,
                        Err(_) => {
                            Logger::error(
                                "A send panicked. The message was dropped; the worker continues.",
                            );
                            account.stats.queued.fetch_sub(1, Ordering::Relaxed);
                            account.stats.failed.fetch_add(1, Ordering::Relaxed);
                            SendResult {
                                error: Some(
                                    EmailError::SendFailed
                                        .because("the send failed with an internal error"),
                                ),
                                ..failed
                            }
                        }
                    };

                    let _ = results.send(result);
                    in_flight.fetch_sub(1, Ordering::Relaxed);
                }
            });
        }
    }
}

impl Drop for SendManager {
    fn drop(&mut self) {
        // Wakes every worker so it retires instead of parking forever.
        let (lock, wake) = &*self.queue;
        if let Ok(mut queue) = lock.lock() {
            queue.closed = true;
        }
        wake.notify_all();
    }
}

/// Blocks until a job is eligible, or returns `None` once the queue closes.
fn next_entry(shared: &Shared) -> Option<Entry> {
    let (lock, wake) = &**shared;
    // A poisoned lock means a worker panicked holding it: retire.
    let mut queue = lock.lock().ok()?;
    loop {
        if queue.closed {
            return None;
        }
        let now = Instant::now();
        let wait = match queue.pick(now) {
            Ok(entry) => return Some(entry),
            Err(Some(at)) => at.saturating_duration_since(now).min(IDLE_WAIT),
            Err(None) => IDLE_WAIT,
        };
        queue = wake.wait_timeout(queue, wait).ok()?.0;
    }
}

/// Runs one attempt. Returns the final result, or `None` when the job went
/// back into the queue for a retry. Worker thread only — never touches the
/// AMX.
fn attempt(mut entry: Entry, shared: &Shared) -> Option<SendResult> {
    let account = entry.job.account.clone();
    if account.dry_run {
        let outcome = match &entry.job.job {
            // Nothing to prove when nothing is sent.
            Job::Test => Ok(()),
            #[cfg(test)]
            Job::Panic => Ok(()),
            Job::Built(message) => write_dry_run(message),
            Job::Send(draft) => {
                message::build(draft, &account.from).and_then(|m| write_dry_run(&m))
            }
        };
        return Some(finish(entry, outcome));
    }

    #[cfg(test)]
    if matches!(entry.job.job, Job::Panic) {
        panic!("a job that panics");
    }

    let outcome = match &entry.job.job {
        #[cfg(test)]
        Job::Panic => unreachable!("handled above"),
        Job::Test => match account.transport.test_connection() {
            Ok(true) => Ok(()),
            // The session opened but the relay did not answer the probe: as
            // unusable as an outright error.
            Ok(false) => {
                let fail = EmailError::ConnectionFailed
                    .because("the relay did not respond to the connection test");
                return Some(finish(entry, Err(fail)));
            }
            Err(error) => Err(error),
        },
        Job::Built(message) => account.transport.send(message).map(drop),
        // Reads attachment files, so it belongs on a worker. Done once: the
        // built message replaces the draft for any retry.
        Job::Send(draft) => match message::build(draft, &account.from) {
            Ok(message) => {
                let sent = account.transport.send(&message).map(drop);
                entry.job.job = Job::Built(Box::new(message));
                sent
            }
            Err(fail) => return Some(finish(entry, Err(fail))),
        },
    };

    let error = match outcome {
        Ok(()) => {
            if let Ok(mut gate) = account.gate.lock() {
                gate.recovered();
            }
            return Some(finish(entry, Ok(())));
        }
        Err(error) => error,
    };

    let code = classify(&error);
    let now = Instant::now();
    if code == EmailError::ConnectionFailed && !entry.is_test() {
        // The relay, not this message: hold the whole account back so the
        // rest of its queue waits instead of hitting a dead relay too.
        if let Ok(mut gate) = account.gate.lock() {
            gate.pause(now, RETRY_DELAY);
        }
    }

    if entry.retries_left == 0 || !is_retryable(&error) {
        return Some(finish(entry, Err(code.because(error.to_string()))));
    }
    if code != EmailError::ConnectionFailed {
        entry.not_before = now + entry.delay;
        entry.delay = entry.delay.saturating_mul(2);
    }
    entry.retries_left -= 1;

    let (lock, wake) = &**shared;
    if let Ok(mut queue) = lock.lock() {
        queue.entries.push(entry);
    }
    wake.notify_all();
    None
}

/// Writes a message to `logs/dry-run/` instead of sending it.
///
/// The file is the real thing — headers, encoded bodies, attachments — so it
/// can be opened in a mail client to see exactly what the relay would have
/// received.
fn write_dry_run(message: &lettre::Message) -> Result<(), Fail> {
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let dir = std::path::Path::new("logs").join("dry-run");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let path = dir.join(format!(
        "{stamp}-{}.eml",
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(&path, message.formatted()))
        .map_err(|e| {
            EmailError::SendFailed.because(format!("could not write '{}': {e}", path.display()))
        })?;

    crate::logger::Logger::info(&format!("Dry run: message written to {}", path.display()));
    Ok(())
}

/// Settles the account's counters and produces the callback's result.
fn finish(entry: Entry, outcome: Result<(), Fail>) -> SendResult {
    let stats = &entry.job.account.stats;
    stats.queued.fetch_sub(1, Ordering::Relaxed);
    let counter = if outcome.is_ok() {
        &stats.sent
    } else {
        &stats.failed
    };
    counter.fetch_add(1, Ordering::Relaxed);

    SendResult {
        account_id: entry.job.account_id,
        error: outcome.err(),
        recipient: entry.recipient,
        callback: entry.job.callback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::AccountManager;
    use crate::options::{EmailOptions, Encryption};
    use crate::settings::Settings;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::AtomicU32;

    /// A relay just capable enough for lettre: every command succeeds except
    /// `RCPT TO`, which answers from `rcpt` in turn (then 250 once it runs
    /// out). Counts the `RCPT` commands it saw, which is how a test tells a
    /// retry from a single attempt.
    struct FakeRelay {
        port: u16,
        rcpt_seen: Arc<AtomicU32>,
    }

    impl FakeRelay {
        fn start(rcpt: &[&'static str]) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().expect("addr").port();
            let rcpt_seen = Arc::new(AtomicU32::new(0));
            let replies = Arc::new(Mutex::new(rcpt.to_vec()));

            let seen = rcpt_seen.clone();
            thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let (seen, replies) = (seen.clone(), replies.clone());
                    thread::spawn(move || serve(stream, &seen, &replies));
                }
            });

            Self { port, rcpt_seen }
        }

        fn rcpts(&self) -> u32 {
            self.rcpt_seen.load(Ordering::Relaxed)
        }
    }

    fn serve(stream: TcpStream, seen: &AtomicU32, replies: &Mutex<Vec<&'static str>>) {
        let mut out = stream.try_clone().expect("clone");
        let mut lines = BufReader::new(stream).lines();
        let mut say = |reply: &str| out.write_all(format!("{reply}\r\n").as_bytes());

        let _ = say("220 fake ESMTP");
        while let Some(Ok(line)) = lines.next() {
            let verb = line.get(..4).unwrap_or("").to_ascii_uppercase();
            let reply = match verb.as_str() {
                "EHLO" => "250 fake",
                "RCPT" => {
                    seen.fetch_add(1, Ordering::Relaxed);
                    let mut queue = replies.lock().expect("lock");
                    if queue.is_empty() {
                        "250 ok"
                    } else {
                        queue.remove(0)
                    }
                }
                "DATA" => {
                    let _ = say("354 go on");
                    for body in lines.by_ref() {
                        if body.map(|l| l == ".").unwrap_or(true) {
                            break;
                        }
                    }
                    "250 queued"
                }
                "QUIT" => {
                    let _ = say("221 bye");
                    return;
                }
                _ => "250 ok",
            };
            if say(reply).is_err() {
                return;
            }
        }
    }

    fn account_on(port: u16, options: EmailOptions) -> AccountHandle {
        let mut mgr = AccountManager::new();
        let (id, _) = mgr
            .connect(&Settings {
                host: "127.0.0.1".into(),
                user: String::new(),
                password: String::new(),
                options: EmailOptions {
                    port: Some(port),
                    encryption: Encryption::None,
                    from_address: "bot@example.com".into(),
                    timeout_secs: 5,
                    ..options
                },
            })
            .expect("builds");
        mgr.handle(id).expect("open")
    }

    fn retrying(retries: u32) -> EmailOptions {
        EmailOptions {
            retries,
            ..EmailOptions::default()
        }
    }

    fn job(account: &AccountHandle, priority: Priority, to: &str) -> SendJob {
        let owner = samp::amx::AmxIdent::from(std::ptr::null_mut::<samp::raw::types::AMX>());
        let mut draft = MessageDraft::new(1, owner);
        draft
            .to
            .push(crate::address::parse_mailbox(to, "").expect("valid"));
        draft.subject = "Hello".into();
        draft.text = "Body".into();
        SendJob {
            account_id: 1,
            account: account.clone(),
            job: Job::Send(Box::new(draft)),
            priority,
            callback: CallbackInfo::empty(),
        }
    }

    /// Submits and waits for every result, through the real workers.
    fn deliver_all(jobs: Vec<SendJob>) -> Vec<SendResult> {
        let mut mgr = SendManager::new();
        let count = jobs.len();
        for job in jobs {
            mgr.submit(job).expect("queued");
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut results = Vec::new();
        while results.len() < count && Instant::now() < deadline {
            results.extend(mgr.poll_results());
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(results.len(), count, "timed out waiting for results");
        results
    }

    fn deliver(job: SendJob) -> SendResult {
        deliver_all(vec![job]).remove(0)
    }

    // --- End to end, against the fake relay ----------------------------------

    #[test]
    fn a_message_is_delivered_end_to_end() {
        let relay = FakeRelay::start(&[]);
        let account = account_on(relay.port, retrying(0));

        let result = deliver(job(&account, Priority::Normal, "player@example.com"));
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(result.recipient, "player@example.com");
        assert_eq!(account.stats.get(), (0, 1, 0));
    }

    #[test]
    fn a_dry_run_writes_the_message_instead_of_sending_it() {
        // Port 1: nothing is listening, so a success proves nothing was sent.
        let account = account_on(
            1,
            EmailOptions {
                dry_run: true,
                ..EmailOptions::default()
            },
        );

        let before = std::fs::read_dir("logs/dry-run")
            .map(|d| d.count())
            .unwrap_or(0);
        let result = deliver(job(&account, Priority::Normal, "player@example.com"));
        assert!(result.error.is_none(), "{:?}", result.error);

        let after = std::fs::read_dir("logs/dry-run").expect("written").count();
        assert_eq!(after, before + 1);
        assert_eq!(account.stats.get(), (0, 1, 0));
    }

    #[test]
    fn a_temporary_rejection_is_retried_until_it_passes() {
        let relay = FakeRelay::start(&["451 try later"]);
        let account = account_on(relay.port, retrying(2));

        let result = deliver(job(&account, Priority::Normal, "player@example.com"));
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(relay.rcpts(), 2);
    }

    #[test]
    fn a_permanent_rejection_is_not_retried() {
        let relay = FakeRelay::start(&["550 no such user"]);
        let account = account_on(relay.port, retrying(3));

        let (code, detail) = deliver(job(&account, Priority::Normal, "player@example.com"))
            .error
            .expect("fails");
        assert_eq!(code, EmailError::SendFailed);
        assert!(detail.contains("no such user"));
        assert_eq!(relay.rcpts(), 1);
        assert_eq!(account.stats.get(), (0, 0, 1));
    }

    #[test]
    fn retries_give_up_with_the_last_error() {
        let relay = FakeRelay::start(&["451 busy", "451 busy", "451 still busy"]);
        let account = account_on(relay.port, retrying(1));

        let (code, _) = deliver(job(&account, Priority::Normal, "player@example.com"))
            .error
            .expect("fails");
        assert_eq!(code, EmailError::SendFailed);
        assert_eq!(relay.rcpts(), 2);
    }

    #[test]
    fn an_unreachable_relay_pauses_the_account_and_reports_once() {
        // Bound and dropped: nothing listens on the port any more.
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("bind")
            .port();
        let account = account_on(port, retrying(1));

        let (code, _) = deliver(job(&account, Priority::Normal, "player@example.com"))
            .error
            .expect("fails");
        assert_eq!(code, EmailError::ConnectionFailed);
        // One pause per failed attempt: the retry waited for the gate rather
        // than a delay of its own.
        assert_eq!(account.gate.lock().expect("gate").failures(), 2);
    }

    #[test]
    fn a_connection_test_ignores_the_pause_and_reports_now() {
        let relay = FakeRelay::start(&[]);
        let account = account_on(relay.port, retrying(0));
        account
            .gate
            .lock()
            .expect("gate")
            .pause(Instant::now(), Duration::from_secs(60));

        let result = deliver(SendJob {
            job: Job::Test,
            ..job(&account, Priority::High, "x@example.com")
        });
        assert!(result.error.is_none(), "{:?}", result.error);
        assert!(result.recipient.is_empty());
    }

    // --- Scheduling, without the network --------------------------------------

    fn queue_of(entries: Vec<(SendJob, Duration)>) -> Queue {
        let now = Instant::now();
        let mut queue = Queue::default();
        for (job, age) in entries {
            let enqueued = now - age;
            let seq = queue.next_seq;
            queue.next_seq += 1;
            queue.entries.push(Entry {
                deadline: enqueued + job.priority.allowance(),
                job,
                recipient: String::new(),
                seq,
                not_before: enqueued,
                retries_left: 0,
                delay: RETRY_DELAY,
            });
        }
        queue
    }

    fn picked(queue: &mut Queue) -> String {
        let Ok(entry) = queue.pick(Instant::now()) else {
            panic!("expected an eligible job");
        };
        match entry.job.job {
            Job::Send(draft) => draft.primary_recipient(),
            _ => String::new(),
        }
    }

    #[test]
    fn higher_priority_goes_first_and_ties_are_first_come() {
        let account = account_on(1, EmailOptions::default());
        let fresh = Duration::ZERO;
        let mut queue = queue_of(vec![
            (job(&account, Priority::Low, "low@x.com"), fresh),
            (job(&account, Priority::Normal, "normal1@x.com"), fresh),
            (job(&account, Priority::High, "high@x.com"), fresh),
            (job(&account, Priority::Normal, "normal2@x.com"), fresh),
        ]);
        assert_eq!(picked(&mut queue), "high@x.com");
        assert_eq!(picked(&mut queue), "normal1@x.com");
        assert_eq!(picked(&mut queue), "normal2@x.com");
        assert_eq!(picked(&mut queue), "low@x.com");
    }

    #[test]
    fn a_low_job_that_waited_long_enough_beats_a_fresh_normal() {
        let account = account_on(1, EmailOptions::default());
        let mut queue = queue_of(vec![
            (
                job(&account, Priority::Normal, "fresh@x.com"),
                Duration::ZERO,
            ),
            (
                job(&account, Priority::Low, "old@x.com"),
                Duration::from_secs(601),
            ),
        ]);
        assert_eq!(picked(&mut queue), "old@x.com");
    }

    #[test]
    fn a_rate_limited_account_holds_back_only_its_own_mail() {
        let slow = account_on(
            1,
            EmailOptions {
                rate_limit: 1,
                ..EmailOptions::default()
            },
        );
        let other = account_on(1, EmailOptions::default());
        let mut queue = queue_of(vec![
            (job(&slow, Priority::High, "slow1@x.com"), Duration::ZERO),
            (job(&slow, Priority::High, "slow2@x.com"), Duration::ZERO),
            (job(&other, Priority::Low, "other@x.com"), Duration::ZERO),
        ]);
        assert_eq!(picked(&mut queue), "slow1@x.com");
        // slow2 now waits a minute for its slot; the other account's mail,
        // though LOW, is not stuck behind it.
        assert_eq!(picked(&mut queue), "other@x.com");
        let Err(Some(wake)) = queue.pick(Instant::now()) else {
            panic!("expected slow2 to wait for its slot");
        };
        assert!(wake > Instant::now() + Duration::from_secs(50));
    }

    #[test]
    fn low_priority_is_refused_first_and_high_never() {
        let account = account_on(
            1,
            EmailOptions {
                queue_limit: 8,
                ..EmailOptions::default()
            },
        );
        account.stats.queued.store(6, Ordering::Relaxed);
        let refused = |p| SendManager::check_room(&account, p).is_err();
        assert!(refused(Priority::Low));
        assert!(!refused(Priority::Normal));

        account.stats.queued.store(8, Ordering::Relaxed);
        assert!(refused(Priority::Normal));
        assert!(!refused(Priority::High));
    }

    #[test]
    fn a_panicking_send_is_reported_and_the_pool_survives() {
        let relay = FakeRelay::start(&[]);
        let account = account_on(relay.port, retrying(0));

        let results = deliver_all(vec![
            SendJob {
                job: Job::Panic,
                ..job(&account, Priority::High, "boom@example.com")
            },
            job(&account, Priority::Normal, "player@example.com"),
        ]);

        assert_eq!(results.iter().filter(|r| r.error.is_some()).count(), 1);
        // The second one still went out, so no worker retired.
        assert_eq!(results.iter().filter(|r| r.error.is_none()).count(), 1);
        assert_eq!(account.stats.get(), (0, 1, 1));
    }

    #[test]
    fn a_full_queue_refuses_before_spawning_anything() {
        let account = account_on(1, EmailOptions::default());
        account
            .stats
            .queued
            .store(account.queue_limit, Ordering::Relaxed);

        let mut mgr = SendManager::new();
        let Err((code, _)) = mgr.submit(job(&account, Priority::Normal, "a@x.com")) else {
            panic!("expected a full queue to refuse");
        };
        assert_eq!(code, EmailError::QueueFull);
        assert_eq!(mgr.pending_count(), 0);
        assert!(!mgr.started);
    }

    #[test]
    fn priorities_round_trip_through_the_include_values() {
        assert_eq!(Priority::from_i32(-1), Some(Priority::Low));
        assert_eq!(Priority::from_i32(0), Some(Priority::Normal));
        assert_eq!(Priority::from_i32(1), Some(Priority::High));
        assert_eq!(Priority::from_i32(2), None);
    }
}
