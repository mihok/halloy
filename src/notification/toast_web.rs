//! Browser notifications (web port) via the Web Notifications API.
//! Replaces toast.rs (notify-rust) on wasm32.

use data::config::actions::NotificationAction;

/// Ask for permission early so the first notification isn't lost.
pub fn prepare() {
    if web_sys::Notification::permission()
        == web_sys::NotificationPermission::Default
    {
        let _ = web_sys::Notification::request_permission();
    }
}

pub struct Toast {
    title: String,
    body: String,
}

impl Toast {
    pub fn new(
        title: &str,
        subtitle: Option<&str>,
        body: &str,
        _has_buffer_context: bool,
        _notification_action: NotificationAction,
    ) -> Self {
        let title = match subtitle {
            Some(subtitle) => format!("{title} — {subtitle}"),
            None => title.to_string(),
        };
        Self {
            title,
            body: body.to_string(),
        }
    }

    pub fn show(self) {
        if web_sys::Notification::permission()
            != web_sys::NotificationPermission::Granted
        {
            return;
        }
        let options = web_sys::NotificationOptions::new();
        options.set_body(&self.body);
        if let Err(error) =
            web_sys::Notification::new_with_options(&self.title, &options)
        {
            log::error!("error showing notification: {error:?}");
        }
    }
}

#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub enum Action {
    ActivateApplication,
    OpenBuffer,
    Dismiss,
}
