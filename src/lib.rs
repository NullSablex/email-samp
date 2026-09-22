//! email_samp — SMTP mail from a SA-MP / Open Multiplayer server.
//!
//! Sending a mail is blocking work measured in hundreds of milliseconds, so
//! every native here returns immediately and reports through a callback on the
//! next tick. Nothing touches the network on the main thread.

mod account;
mod address;
mod callback;
mod config;
mod error;
mod format;
mod logger;
mod message;
mod natives;
mod options;
mod plugin;
mod sandbox;
mod sender;
mod settings;
mod template;

use plugin::EmailPlugin;
use samp::initialize_plugin;

initialize_plugin!(
    natives: [
        // Accounts
        EmailPlugin::email_setup,
        EmailPlugin::email_connect,
        EmailPlugin::email_close,
        EmailPlugin::email_is_account,
        EmailPlugin::email_status,
        EmailPlugin::email_test,
        EmailPlugin::email_stats,
        // Errors
        EmailPlugin::email_errno,
        EmailPlugin::email_error,
        // Messages
        EmailPlugin::email_new,
        EmailPlugin::email_destroy,
        EmailPlugin::email_is_message,
        EmailPlugin::email_set_from,
        EmailPlugin::email_set_reply_to,
        EmailPlugin::email_add_to,
        EmailPlugin::email_add_cc,
        EmailPlugin::email_add_bcc,
        EmailPlugin::email_set_subject,
        EmailPlugin::email_set_body,
        EmailPlugin::email_set_html,
        EmailPlugin::email_set_priority,
        EmailPlugin::email_add_header,
        EmailPlugin::email_attach,
        EmailPlugin::email_embed,
        EmailPlugin::email_attach_data,
        EmailPlugin::email_send,
        EmailPlugin::email_send_to,
        EmailPlugin::email_send_file,
        // Templates
        EmailPlugin::email_set_template,
        EmailPlugin::email_set_var,
        // Utility
        EmailPlugin::email_is_valid_address,
        EmailPlugin::email_log,
        EmailPlugin::email_limit,
        EmailPlugin::email_force_reload_templates,
    ],
    {
        samp::plugin::enable_tick();
        return EmailPlugin::new();
    }
);
