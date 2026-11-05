use data::Config;
use data::config::{platform_specific, sidebar};

/// Browser build (web port). A tab can't open OS windows, quit itself, or
/// reach a desktop filesystem, so those features are hidden; the config is
/// managed by the host page (see src/web.rs).
pub const IS_WEB: bool = cfg!(target_arch = "wasm32");

/// Panes can pop out into their own OS window (desktop only).
pub const SUPPORTS_WINDOWS: bool = !IS_WEB;

/// Browser build: where "Sign out" goes, from the hosting page's
/// `<meta name="halloy-logout-url">` (e.g. an SSO logout endpoint). Only
/// same-origin paths are accepted. `None` hides the menu item, and always on
/// desktop.
pub fn logout_url() -> Option<&'static str> {
    #[cfg(target_arch = "wasm32")]
    {
        static URL: std::sync::OnceLock<Option<String>> =
            std::sync::OnceLock::new();
        URL.get_or_init(|| {
            crate::web::meta("halloy-logout-url")
                .filter(|url| url.starts_with('/') && !url.starts_with("//"))
        })
        .as_deref()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// Browser build: leave the app for the logout URL in this tab.
pub fn sign_out() {
    #[cfg(target_arch = "wasm32")]
    if let (Some(url), Some(window)) = (logout_url(), web_sys::window()) {
        let _ = window.location().assign(url);
    }
}

/// Returns the content padding based on platform and configuration..
pub fn content_padding(config: &Config) -> u32 {
    // On macOS the content is either embedded into titlebar or padded below it.
    if cfg!(target_os = "macos") {
        let padding = match config.platform_specific.macos.content_padding {
            platform_specific::TitlebarPadding::EmbeddedContent => 0,
            platform_specific::TitlebarPadding::PaddedContent => 20,
        };

        match config.sidebar.position {
            sidebar::Position::Left => padding,
            sidebar::Position::Right => padding,
            sidebar::Position::Top => 0,
            sidebar::Position::Bottom => padding,
        }
    } else {
        0
    }
}

pub fn popped_out_window_padding(config: &Config) -> u32 {
    if cfg!(target_os = "macos") {
        match config.platform_specific.macos.content_padding {
            platform_specific::TitlebarPadding::EmbeddedContent => 30,
            platform_specific::TitlebarPadding::PaddedContent => 0,
        }
    } else {
        0
    }
}

/// Returns the sidebar padding based on platform and configuration..
pub fn sidebar_padding(config: &Config) -> u32 {
    if cfg!(target_os = "macos") {
        let padding = match config.platform_specific.macos.sidebar_padding {
            platform_specific::TitlebarPadding::EmbeddedContent => 0,
            platform_specific::TitlebarPadding::PaddedContent => 20,
        };

        match config.sidebar.position {
            sidebar::Position::Left => padding,
            sidebar::Position::Right => padding,
            sidebar::Position::Top => padding,
            sidebar::Position::Bottom => 0,
        }
    } else {
        0
    }
}
