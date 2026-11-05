use std::ffi::OsStr;
use std::io;

#[cfg(target_os = "macos")]
pub fn open(target: impl AsRef<OsStr>) -> io::Result<()> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSString, NSURL};

    let target = target.as_ref();

    if let Some(target_str) = target.to_str() {
        let target_string = NSString::from_str(target_str);
        let url = if url::Url::parse(target_str).is_ok() {
            NSURL::URLWithString(&target_string)
        } else {
            Some(NSURL::fileURLWithPath(&target_string))
        };

        if let Some(url) = url
            && NSWorkspace::sharedWorkspace().openURL(&url)
        {
            return Ok(());
        }
    }

    open::that_detached(target)
}

/// Browser: open http(s) links in a new tab; there are no local files or
/// folders to reveal.
#[cfg(target_arch = "wasm32")]
pub fn open(target: impl AsRef<OsStr>) -> io::Result<()> {
    let target = target.as_ref().to_string_lossy();
    let is_web = url::Url::parse(&target)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https"));
    if !is_web {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("can't open {target} in the browser"),
        ));
    }
    web_sys::window()
        .and_then(|window| {
            window
                .open_with_url_and_target_and_features(
                    &target,
                    "_blank",
                    "noopener,noreferrer",
                )
                .ok()
        })
        .map(|_| ())
        .ok_or_else(|| io::Error::other("window.open failed"))
}

#[cfg(all(not(target_os = "macos"), not(target_arch = "wasm32")))]
pub fn open(target: impl AsRef<OsStr>) -> io::Result<()> {
    open::that_detached(target)
}
