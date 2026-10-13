//! Native desktop notifications (`notify-rust`): Notification Center on
//! macOS, toasts on Windows, D-Bus on Linux. Used by `App` to tell the
//! user a long task finished or something failed while the window is in
//! the background. Off in snapshots and tests, and when the user sets
//! `notifications = false`.

use notify_rust::Notification;

/// Shows a notification with `summary` as the title and `body` below it.
/// Runs on its own thread because showing one can block (macOS waits for
/// the notification server); failures only go to the error log.
pub fn send(summary: String, body: String) {
    if crate::settings::user_config_dir().is_none() || !crate::settings::settings().notifications {
        return;
    }
    std::thread::spawn(move || {
        let shown = Notification::new()
            .appname(crate::APP_NAME)
            .summary(&summary)
            .body(&body)
            .show();
        if let Err(error) = shown {
            crate::log::warn(format_args!("could not show notification: {error}"));
        }
    });
}
