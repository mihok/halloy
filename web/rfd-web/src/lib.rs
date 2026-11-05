//! `rfd`-compatible file dialogs for the browser (web port).
//!
//! * `pick_file` / `pick_files` open the browser's file picker and copy each
//!   chosen file into the in-memory filesystem (`data::platform::fs`) under
//!   `/halloy/picked/`, so code that reads the returned path works as on
//!   desktop.
//! * `save_file` returns `None` for now: the browser has no paths to save to.
//!   Saving will become a download.
#![cfg(target_arch = "wasm32")]

use std::path::{Path, PathBuf};

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

#[derive(Debug, Clone, Default)]
pub struct AsyncFileDialog {
    accept: Vec<String>,
}

impl AsyncFileDialog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_file_name(self, _name: impl Into<String>) -> Self {
        self
    }

    pub fn set_directory(self, _path: impl AsRef<Path>) -> Self {
        self
    }

    pub fn set_title(self, _title: impl Into<String>) -> Self {
        self
    }

    pub fn add_filter(
        mut self,
        _name: impl Into<String>,
        extensions: &[impl ToString],
    ) -> Self {
        self.accept
            .extend(extensions.iter().map(|e| format!(".{}", e.to_string())));
        self
    }

    pub async fn pick_file(self) -> Option<FileHandle> {
        self.pick(false).await.into_iter().next()
    }

    pub async fn pick_files(self) -> Option<Vec<FileHandle>> {
        let files = self.pick(true).await;
        (!files.is_empty()).then_some(files)
    }

    pub async fn save_file(self) -> Option<FileHandle> {
        log::warn!("saving files isn't supported in the browser yet");
        None
    }

    async fn pick(&self, multiple: bool) -> Vec<FileHandle> {
        let Some(input) = file_input(multiple, &self.accept.join(",")) else {
            return Vec::new();
        };

        // Resolve when the user picks (change) or dismisses (cancel).
        let (sender, receiver) = futures::channel::oneshot::channel::<()>();
        let sender = std::rc::Rc::new(std::cell::RefCell::new(Some(sender)));
        let done = {
            let sender = sender.clone();
            Closure::<dyn FnMut()>::new(move || {
                if let Some(s) = sender.borrow_mut().take() {
                    let _ = s.send(());
                }
            })
        };
        input.set_onchange(Some(done.as_ref().unchecked_ref()));
        let _ = input.add_event_listener_with_callback(
            "cancel",
            done.as_ref().unchecked_ref(),
        );
        input.click();
        let _ = receiver.await;
        drop(done);

        let mut handles = Vec::new();
        let Some(files) = input.files() else {
            return handles;
        };
        let dir = PathBuf::from("/halloy/picked");
        let _ = data::platform::fs::create_dir_all(&dir).await;
        for i in 0..files.length() {
            let Some(file) = files.get(i) else { continue };
            match read(&file).await {
                Ok(bytes) => {
                    let path = dir.join(sanitize(&file.name()));
                    if data::platform::fs::write(&path, bytes).await.is_ok() {
                        handles.push(FileHandle(path));
                    }
                }
                Err(e) => log::error!("reading {}: {e:?}", file.name()),
            }
        }
        handles
    }
}

fn file_input(
    multiple: bool,
    accept: &str,
) -> Option<web_sys::HtmlInputElement> {
    let document = web_sys::window()?.document()?;
    let input = document
        .create_element("input")
        .ok()?
        .dyn_into::<web_sys::HtmlInputElement>()
        .ok()?;
    input.set_type("file");
    input.set_multiple(multiple);
    if !accept.is_empty() {
        input.set_accept(accept);
    }
    Some(input)
}

async fn read(file: &web_sys::File) -> Result<Vec<u8>, wasm_bindgen::JsValue> {
    let buffer =
        wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

/// Keep only the final path component of a browser-supplied name.
fn sanitize(name: &str) -> String {
    let name = name.rsplit(['/', '\\']).next().unwrap_or("file");
    if name.is_empty() || name == "." || name == ".." {
        "file".into()
    } else {
        name.into()
    }
}

#[derive(Debug, Clone)]
pub struct FileHandle(PathBuf);

impl FileHandle {
    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn file_name(&self) -> String {
        self.0
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}
