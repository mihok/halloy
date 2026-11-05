use data::Config;
use data::config::{platform_specific, sidebar};

/// Browser build (web port). A tab can't open OS windows, quit itself, or
/// reach a desktop filesystem, so those features are hidden; the config is
/// managed by the host page (see src/web.rs).
pub const IS_WEB: bool = cfg!(target_arch = "wasm32");

/// Panes can pop out into their own OS window (desktop only).
pub const SUPPORTS_WINDOWS: bool = !IS_WEB;

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
