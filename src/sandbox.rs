//! Keeps the files a message reads inside the server folder.
//!
//! An attachment, an embedded image or a template ends up *inside a mail*, so
//! a path that escapes the folder is a way to mail a file out: a gamemode
//! building `logs/<nickname>.txt` would otherwise send `server.cfg`.
//!
//! The rule: `..` is plain path arithmetic and is fine while the result stays
//! inside; no part of the path may be a symlink (or a Windows junction),
//! wherever it points; it must end at a regular file. What is read afterwards
//! is the *checked* path, so `link/../x` never resolves through the link.
//!
//! The check runs when the native is called and again in the worker, which is
//! what [`read`] is for. The configuration file and `tls_ca` are exempt: only
//! the operator sets them and they never become part of a mail.

use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use crate::error::{EmailError, Fail};

/// Checks `path` (relative to the server folder, or absolute inside it) and
/// returns it as an absolute path. `code` is the error reported on failure.
pub fn resolve(path: &Path, code: EmailError) -> Result<PathBuf, Fail> {
    let fail = |why: String| code.because(format!("'{}' {why}", path.display()));

    let root = std::env::current_dir()
        .and_then(|dir| dir.canonicalize())
        .map_err(|e| {
            fail(format!(
                "cannot be checked: the server folder is unreadable ({e})"
            ))
        })?;

    // An absolute path replaces the root when joined, so it has to name a
    // place under the root to get past the strip below.
    let full = normalize(&root.join(path));
    let inside = full
        .strip_prefix(&root)
        .map_err(|_| fail(String::from("is outside the server folder")))?;

    // Walk it one part at a time, looking at each part itself rather than at
    // what it points to.
    let mut current = root.clone();
    for part in inside.components() {
        current.push(part);
        let meta = std::fs::symlink_metadata(&current)
            .map_err(|e| fail(format!("is not readable: {e}")))?;
        if meta.file_type().is_symlink() {
            return Err(fail(format!(
                "goes through a symlink ('{}')",
                part.as_os_str().to_string_lossy()
            )));
        }
    }

    if !current.is_file() {
        return Err(fail(String::from("is not a regular file")));
    }
    Ok(current)
}

/// Collapses `.` and `..` without touching the disk. Climbing above the
/// filesystem root just stays there, like the OS does.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Checks `path` and reads it, in one step.
///
/// Between the check and the `open` there is a moment in which the file could
/// be replaced by a symlink. So the opened file's identity is compared with
/// the checked one's (same device and inode on Unix), and a mismatch is
/// refused. This is the call every read of a mail's files goes through.
pub fn read(path: &Path, code: EmailError) -> Result<Vec<u8>, Fail> {
    let checked = resolve(path, code)?;
    let fail = |why: String| code.because(format!("'{}' {why}", checked.display()));

    let mut file = File::open(&checked).map_err(|e| fail(format!("could not be read: {e}")))?;
    if !same_file(&file, &checked) {
        return Err(fail(String::from("was replaced while it was being read")));
    }

    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| fail(format!("could not be read: {e}")))?;
    Ok(bytes)
}

/// Whether the open file is the same one the path names right now.
#[cfg(unix)]
fn same_file(file: &File, path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    let (Ok(open), Ok(named)) = (file.metadata(), std::fs::symlink_metadata(path)) else {
        return false;
    };
    open.dev() == named.dev() && open.ino() == named.ino()
}

/// Windows has no cheap equivalent, and opening the file already followed the
/// path that was checked; the walk above is the guarantee there.
#[cfg(not(unix))]
fn same_file(_file: &File, _path: &Path) -> bool {
    true
}

/// A scratch directory inside the crate (the tests' working directory), so
/// test files are inside the "server folder".
#[cfg(test)]
pub fn test_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from("target").join("sandbox-tests").join(name);
    std::fs::create_dir_all(&dir).expect("test dir");
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: EmailError = EmailError::AttachmentFailed;

    fn refused(path: &Path) -> String {
        match resolve(path, CODE) {
            Ok(p) => panic!(
                "expected {} to be refused, got {}",
                path.display(),
                p.display()
            ),
            Err((code, message)) => {
                assert_eq!(code, CODE);
                message
            }
        }
    }

    #[test]
    fn a_file_inside_the_folder_is_allowed() {
        let dir = test_dir("inside");
        std::fs::write(dir.join("a.txt"), "x").expect("write");

        let resolved = resolve(&dir.join("a.txt"), CODE).expect("allowed");
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with("a.txt"));
    }

    #[test]
    fn an_absolute_path_inside_the_folder_is_allowed() {
        let dir = test_dir("absolute_in");
        std::fs::write(dir.join("a.txt"), "x").expect("write");
        let absolute = std::env::current_dir()
            .expect("cwd")
            .join(dir)
            .join("a.txt");
        assert!(resolve(&absolute, CODE).is_ok());
    }

    #[test]
    fn dot_dot_that_stays_inside_is_fine() {
        let dir = test_dir("dotdot");
        std::fs::write(dir.join("b.txt"), "x").expect("write");

        let resolved =
            resolve(&dir.join("..").join("dotdot").join("b.txt"), CODE).expect("allowed");
        assert!(resolved.ends_with("dotdot/b.txt"));
    }

    #[test]
    fn dot_dot_that_climbs_out_is_refused() {
        assert!(
            refused(Path::new("../../../../etc/hostname")).contains("outside the server folder")
        );
        assert!(refused(&test_dir("climb").join("../../../../etc/hostname")).contains("outside"));
    }

    #[test]
    fn an_absolute_path_outside_is_refused() {
        let outside = std::env::temp_dir().join("email_samp_sandbox_outside.txt");
        std::fs::write(&outside, "secret").expect("write");
        assert!(refused(&outside).contains("outside the server folder"));
    }

    #[test]
    fn a_directory_is_not_a_file() {
        assert!(refused(&test_dir("adir")).contains("not a regular file"));
    }

    #[test]
    fn a_missing_file_says_so() {
        assert!(refused(&test_dir("missing").join("nope.txt")).contains("not readable"));
    }

    #[cfg(unix)]
    fn symlink(target: &Path, link: &Path) {
        let _ = std::fs::remove_file(link);
        std::os::unix::fs::symlink(target, link).expect("symlink");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_file_is_refused_wherever_it_points() {
        let dir = test_dir("symlink_file");
        std::fs::write(dir.join("real.txt"), "x").expect("write");
        let outside = std::env::temp_dir().join("email_samp_sandbox_target.txt");
        std::fs::write(&outside, "secret").expect("write");

        symlink(Path::new("real.txt"), &dir.join("alias.txt"));
        symlink(&outside, &dir.join("innocent.txt"));

        assert!(refused(&dir.join("alias.txt")).contains("symlink ('alias.txt')"));
        assert!(refused(&dir.join("innocent.txt")).contains("symlink"));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_folder_on_the_way_is_refused() {
        let real = test_dir("symlink_dir_real");
        std::fs::write(real.join("c.txt"), "x").expect("write");

        let dir = test_dir("symlink_dir");
        symlink(
            &std::fs::canonicalize(&real).expect("real"),
            &dir.join("shared"),
        );

        assert!(refused(&dir.join("shared").join("c.txt")).contains("symlink ('shared')"));
    }

    #[test]
    fn read_returns_the_contents_of_a_checked_file() {
        let dir = test_dir("read");
        std::fs::write(dir.join("d.txt"), "contents").expect("write");
        assert_eq!(read(&dir.join("d.txt"), CODE).expect("read"), b"contents");
    }

    #[cfg(unix)]
    #[test]
    fn read_refuses_a_file_that_became_a_symlink() {
        let dir = test_dir("read_swapped");
        let path = dir.join("e.txt");
        // The test leaves a symlink behind, so start from a clean file.
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, "ours").expect("write");

        let outside = std::env::temp_dir().join("email_samp_sandbox_swapped.txt");
        std::fs::write(&outside, "secret").expect("write");

        // resolve() accepted the real file; the swap happens before the open.
        assert!(resolve(&path, CODE).is_ok());
        symlink(&outside, &path);

        // The check runs again inside read(), so the swap is caught either as
        // a symlink or as a file that changed underneath.
        assert!(read(&path, CODE).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn dot_dot_after_a_symlink_is_resolved_on_the_path_not_through_the_link() {
        // `link/../file.txt`: the OS would climb from where `link` points. The
        // checked path is `dir/file.txt`, which never touches the link.
        let elsewhere = std::env::temp_dir().join("email_samp_sandbox_elsewhere");
        std::fs::create_dir_all(elsewhere.join("sub")).expect("dir");
        std::fs::write(elsewhere.join("file.txt"), "secret").expect("write");

        let dir = test_dir("dotdot_link");
        std::fs::write(dir.join("file.txt"), "ours").expect("write");
        symlink(&elsewhere.join("sub"), &dir.join("link"));

        let resolved =
            resolve(&dir.join("link").join("..").join("file.txt"), CODE).expect("allowed");
        assert_eq!(std::fs::read_to_string(resolved).expect("read"), "ours");
    }
}
