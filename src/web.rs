//! Browser entry point (web port, slashcheer fork). See web/PORTING.md.
//!
//! The page is served by the same host as the IRC server. The host's login
//! gate (Cloudflare Access + the slashcheer Worker) has already identified
//! the visitor and logs the websocket in as their account, so the seeded
//! config needs no nickname, password or SASL.
//!
//! The config is managed: it's rewritten on every load from the page's meta
//! tags and the defaults below (the config editor is hidden in the browser),
//! so changes to the defaults reach everyone on their next visit. The
//! slashcheer themes are written alongside it.

use std::fmt::Write as _;

use data::platform::fs;
use data::{Config, environment};

pub fn start() {
    console_error_panic_hook::set_once();

    // File logging isn't available; logger::setup adds a console sink.
    let log_stream = match crate::logger::setup(data::config::Logs::default()) {
        Ok(stream) => stream,
        Err(error) => {
            web_sys::console::error_1(
                &format!("logging setup failed: {error}").into(),
            );
            return;
        }
    };
    log::info!(
        "halloy {} (web) has started",
        environment::formatted_version()
    );

    // Ask for notification permission up front.
    crate::notification::prepare();

    wasm_bindgen_futures::spawn_local(async move {
        // Restore saved config, drafts, layout and history before anything
        // reads them. Without browser storage the app still runs, unsaved.
        match fs::persist::init().await {
            Ok(files) => log::info!("restored {files} saved files"),
            Err(error) => log::warn!(
                "browser storage unavailable; nothing will be saved: {error}"
            ),
        }

        if let Err(error) = write_config().await {
            log::error!("could not write the default config: {error}");
        }

        let config_load = Config::load().await;
        let window_load = data::Window::load().await;

        if let Err(error) =
            crate::run(config_load, window_load, None, log_stream)
        {
            log::error!("halloy exited: {error}");
        }
    });
}

/// Themes shipped with the web build: (file stem, contents).
const THEMES: &[(&str, &str)] = &[
    (
        "slashcheer-dark",
        include_str!("../web/themes/slashcheer-dark.toml"),
    ),
    (
        "slashcheer-light",
        include_str!("../web/themes/slashcheer-light.toml"),
    ),
];

/// Write the managed config and themes (replacing earlier versions).
async fn write_config() -> std::io::Result<()> {
    fs::create_dir_all(environment::config_dir()).await?;
    fs::create_dir_all(Config::themes_dir()).await?;
    fs::create_dir_all(environment::data_dir()).await?;
    fs::create_dir_all(environment::cache_dir()).await?;
    for (name, contents) in THEMES {
        let path = Config::themes_dir().join(format!("{name}.toml"));
        fs::write(&path, contents).await?;
    }
    fs::write(Config::path(), default_config()).await
}

fn default_config() -> String {
    let location = web_sys::window().map(|w| w.location());
    let host = location
        .as_ref()
        .and_then(|l| l.hostname().ok())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "localhost".into());
    let secure = location
        .as_ref()
        .and_then(|l| l.protocol().ok())
        .is_none_or(|p| p == "https:");
    let port = location
        .as_ref()
        .and_then(|l| l.port().ok())
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(if secure { 443 } else { 80 });
    let network = meta("halloy-network").unwrap_or_else(|| "irc".into());
    let path =
        meta("halloy-websocket-path").unwrap_or_else(|| "/web/irc".into());

    let mut toml = String::new();
    let _ = writeln!(
        toml,
        "# Managed by the web build and rewritten on every load."
    );
    // Follows the system light/dark setting (prefers-color-scheme).
    let _ = writeln!(
        toml,
        "theme = {{ light = \"slashcheer-light\", dark = \"slashcheer-dark\" }}"
    );
    // Updates arrive with the page; there's nothing to install.
    let _ = writeln!(toml, "check_for_update_on_launch = false");
    let _ = writeln!(toml);
    let _ = writeln!(toml, "[servers.{}]", toml_key(&network));
    // The server-side gate replaces this with the signed-in account.
    let _ = writeln!(toml, "nickname = \"web\"");
    let _ = writeln!(toml, "server = {}", toml_string(&host));
    let _ = writeln!(toml, "port = {port}");
    let _ = writeln!(toml, "use_tls = {secure}");
    let _ = writeln!(toml, "use_websocket = true");
    let _ = writeln!(toml, "websocket_path = {}", toml_string(&path));
    // Optional auto-join: <meta name="halloy-channels" content="#a, #b">
    if let Some(channels) = meta("halloy-channels") {
        let list = channels
            .split([',', ' '])
            .filter(|c| !c.is_empty())
            .map(toml_string)
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(toml, "channels = [{list}]");
    }
    toml.push_str(WEB_DEFAULTS);
    toml
}

/// Settings for what a browser tab can and can't do.
const WEB_DEFAULTS: &str = r#"
# DCC needs direct TCP connections, which a browser can't make.
[file_transfer]
enabled = false

# Previews fetch the linked page or image from the browser, which most
# sites refuse (CORS), so they'd mostly fail.
[preview]
enabled = false

# Halloy fades join/part/quit/nick/away lines with an alpha scaled to the
# background's lightness: about 22% on the dark theme and 60% on the light one,
# which makes them unreadable in both. The themes' server-message color
# already marks them as secondary, so keep them solid.
[buffer.server_messages.default]
dimmed = false

# Same for nicknames: offline ones use the theme's offline color as is, and
# away ones keep a fixed 70% so they still read as away without vanishing.
[buffer.nickname]
offline = "solid"
away = { dimmed = 0.7 }

# Tab-like layout: the buffer list runs across the top, and opening a
# channel switches the current pane instead of splitting the screen (right-
# click > "Open in new pane" still splits).
[sidebar]
position = "top"

[actions.sidebar]
buffer = "replace-pane"

# Same for everything else that opens a buffer (joining, messaging someone,
# clicking a channel name or a highlight, notifications).
[actions.buffer]
click_channel_name = "replace-pane"
click_highlight = "replace-pane"
click_channel_discovery = "replace-pane"
join_channel = "replace-pane"
open_internal = "replace-pane"
message_channel = "replace-pane"
message_user = "replace-pane"

[actions.notification]
open_buffer = "replace-pane"

# Browser notifications (permission is asked on first load).
[notifications.direct_message]
show_toast = true
show_content = true

[notifications.highlight]
show_toast = true
show_content = true
"#;

/// Read `<meta name="..." content="...">` from the page.
pub(crate) fn meta(name: &str) -> Option<String> {
    let document = web_sys::window()?.document()?;
    let element = document
        .query_selector(&format!("meta[name=\"{name}\"]"))
        .ok()??;
    element.get_attribute("content").filter(|c| !c.is_empty())
}

fn toml_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn toml_key(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        s.to_string()
    } else {
        toml_string(s)
    }
}
