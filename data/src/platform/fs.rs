//! `tokio::fs` on desktop; an in-memory filesystem in the browser.
//!
//! The browser build has no disk. Config, themes and caches live in this
//! virtual filesystem for the lifetime of the page; persisting it to browser
//! storage (localStorage / OPFS) is a later step of the port.

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::fs::*;
#[cfg(not(target_arch = "wasm32"))]
pub use tokio_stream::wrappers::ReadDirStream;

#[cfg(target_arch = "wasm32")]
pub use self::memory::*;

#[cfg(target_arch = "wasm32")]
mod memory {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};
    use std::ffi::OsString;
    use std::io;
    use std::path::{Component, Path, PathBuf};
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::time::SystemTime;

    struct Vfs {
        files: BTreeMap<PathBuf, (Vec<u8>, SystemTime)>,
        dirs: BTreeSet<PathBuf>,
    }

    thread_local! {
        static VFS: RefCell<Vfs> = RefCell::new(Vfs { files: BTreeMap::new(), dirs: BTreeSet::from([PathBuf::from("/")]) });
    }

    /// Lexically normalize to an absolute path (no symlinks in the VFS).
    fn norm(path: &Path) -> PathBuf {
        let mut out = PathBuf::from("/");
        for c in path.components() {
            match c {
                Component::Prefix(_)
                | Component::RootDir
                | Component::CurDir => {}
                Component::ParentDir => {
                    out.pop();
                }
                Component::Normal(p) => out.push(p),
            }
        }
        out
    }

    fn not_found(path: &Path) -> io::Error {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} not found", path.display()),
        )
    }

    fn now() -> SystemTime {
        // std's SystemTime::now() panics on wasm32-unknown-unknown.
        let ms = js_now_ms();
        SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(ms as u64)
    }

    fn js_now_ms() -> f64 {
        web_time::SystemTime::now()
            .duration_since(web_time::SystemTime::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_millis() as f64)
    }

    pub async fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        let p = norm(path.as_ref());
        VFS.with_borrow(|v| v.files.get(&p).map(|(b, _)| b.clone()))
            .ok_or_else(|| not_found(&p))
    }

    pub async fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
        String::from_utf8(read(path).await?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub async fn write(
        path: impl AsRef<Path>,
        contents: impl AsRef<[u8]>,
    ) -> io::Result<()> {
        let p = norm(path.as_ref());
        let parent = p
            .parent()
            .map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
        VFS.with_borrow_mut(|v| {
            if !v.dirs.contains(&parent) {
                return Err(not_found(&parent));
            }
            v.files.insert(p, (contents.as_ref().to_vec(), now()));
            Ok(())
        })
    }

    pub async fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
        let p = norm(path.as_ref());
        VFS.with_borrow_mut(|v| {
            for a in p.ancestors() {
                v.dirs.insert(a.to_path_buf());
            }
        });
        Ok(())
    }

    pub async fn create_dir(path: impl AsRef<Path>) -> io::Result<()> {
        let p = norm(path.as_ref());
        VFS.with_borrow_mut(|v| {
            if v.dirs.contains(&p) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    p.display().to_string(),
                ));
            }
            v.dirs.insert(p);
            Ok(())
        })
    }

    pub async fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
        let p = norm(path.as_ref());
        VFS.with_borrow_mut(|v| {
            v.files.remove(&p).map(|_| ()).ok_or_else(|| not_found(&p))
        })
    }

    pub async fn rename(
        from: impl AsRef<Path>,
        to: impl AsRef<Path>,
    ) -> io::Result<()> {
        let (from, to) = (norm(from.as_ref()), norm(to.as_ref()));
        VFS.with_borrow_mut(|v| {
            let f = v.files.remove(&from).ok_or_else(|| not_found(&from))?;
            v.files.insert(to, f);
            Ok(())
        })
    }

    pub async fn try_exists(path: impl AsRef<Path>) -> io::Result<bool> {
        let p = norm(path.as_ref());
        Ok(
            VFS.with_borrow(|v| {
                v.files.contains_key(&p) || v.dirs.contains(&p)
            }),
        )
    }

    #[derive(Debug, Clone)]
    pub struct Metadata {
        len: u64,
        is_dir: bool,
        modified: SystemTime,
    }

    impl Metadata {
        pub fn len(&self) -> u64 {
            self.len
        }
        pub fn is_empty(&self) -> bool {
            self.len == 0
        }
        pub fn is_dir(&self) -> bool {
            self.is_dir
        }
        pub fn is_file(&self) -> bool {
            !self.is_dir
        }
        pub fn modified(&self) -> io::Result<SystemTime> {
            Ok(self.modified)
        }
    }

    pub async fn metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
        let p = norm(path.as_ref());
        VFS.with_borrow(|v| {
            if let Some((b, t)) = v.files.get(&p) {
                Some(Metadata {
                    len: b.len() as u64,
                    is_dir: false,
                    modified: *t,
                })
            } else if v.dirs.contains(&p) {
                Some(Metadata {
                    len: 0,
                    is_dir: true,
                    modified: SystemTime::UNIX_EPOCH,
                })
            } else {
                None
            }
        })
        .ok_or_else(|| not_found(&p))
    }

    #[derive(Debug, Clone)]
    pub struct DirEntry {
        path: PathBuf,
    }

    impl DirEntry {
        pub fn path(&self) -> PathBuf {
            self.path.clone()
        }
        pub fn file_name(&self) -> OsString {
            self.path
                .file_name()
                .map(ToOwned::to_owned)
                .unwrap_or_default()
        }
    }

    pub struct ReadDir {
        entries: std::vec::IntoIter<DirEntry>,
    }

    impl ReadDir {
        pub async fn next_entry(&mut self) -> io::Result<Option<DirEntry>> {
            Ok(self.entries.next())
        }
    }

    pub async fn read_dir(path: impl AsRef<Path>) -> io::Result<ReadDir> {
        let p = norm(path.as_ref());
        VFS.with_borrow(|v| {
            if !v.dirs.contains(&p) {
                return Err(not_found(&p));
            }
            let children = v
                .files
                .keys()
                .chain(v.dirs.iter())
                .filter(|c| c.parent() == Some(p.as_path()))
                .map(|c| DirEntry { path: c.clone() })
                .collect::<Vec<_>>();
            Ok(ReadDir {
                entries: children.into_iter(),
            })
        })
    }

    /// Mirrors `tokio_stream::wrappers::ReadDirStream`.
    pub struct ReadDirStream(ReadDir);

    impl ReadDirStream {
        pub fn new(read_dir: ReadDir) -> Self {
            Self(read_dir)
        }
    }

    impl futures::Stream for ReadDirStream {
        type Item = io::Result<DirEntry>;
        fn poll_next(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.get_mut().0.entries.next().map(Ok))
        }
    }

    // ---- file handles -------------------------------------------------

    /// An open file in the VFS. Reads and writes go straight to the shared
    /// store, so `tokio::io` helpers (`write_all`, `BufWriter`,
    /// `read_to_end`, ...) work unchanged.
    #[derive(Debug)]
    pub struct File {
        path: PathBuf,
        pos: u64,
        append: bool,
    }

    #[derive(Debug, Clone, Default)]
    pub struct OpenOptions {
        read: bool,
        write: bool,
        append: bool,
        truncate: bool,
        create: bool,
        create_new: bool,
    }

    impl OpenOptions {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn read(&mut self, v: bool) -> &mut Self {
            self.read = v;
            self
        }
        pub fn write(&mut self, v: bool) -> &mut Self {
            self.write = v;
            self
        }
        pub fn append(&mut self, v: bool) -> &mut Self {
            self.append = v;
            self
        }
        pub fn truncate(&mut self, v: bool) -> &mut Self {
            self.truncate = v;
            self
        }
        pub fn create(&mut self, v: bool) -> &mut Self {
            self.create = v;
            self
        }
        pub fn create_new(&mut self, v: bool) -> &mut Self {
            self.create_new = v;
            self
        }

        pub async fn open(&self, path: impl AsRef<Path>) -> io::Result<File> {
            let p = norm(path.as_ref());
            let parent = p
                .parent()
                .map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
            VFS.with_borrow_mut(|v| {
                let exists = v.files.contains_key(&p);
                if self.create_new && exists {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        p.display().to_string(),
                    ));
                }
                if !exists {
                    if !(self.create || self.create_new) {
                        return Err(not_found(&p));
                    }
                    if !v.dirs.contains(&parent) {
                        return Err(not_found(&parent));
                    }
                    v.files.insert(p.clone(), (Vec::new(), now()));
                } else if self.truncate {
                    v.files.insert(p.clone(), (Vec::new(), now()));
                }
                Ok(())
            })?;
            Ok(File {
                path: p,
                pos: 0,
                append: self.append,
            })
        }
    }

    impl File {
        pub async fn open(path: impl AsRef<Path>) -> io::Result<File> {
            OpenOptions::new().read(true).open(path).await
        }
        pub async fn create(path: impl AsRef<Path>) -> io::Result<File> {
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
                .await
        }
        pub fn options() -> OpenOptions {
            OpenOptions::new()
        }
        pub async fn metadata(&self) -> io::Result<Metadata> {
            metadata(&self.path).await
        }
        pub async fn sync_all(&self) -> io::Result<()> {
            Ok(())
        }
        pub async fn set_len(&self, len: u64) -> io::Result<()> {
            VFS.with_borrow_mut(|v| {
                let f = v
                    .files
                    .get_mut(&self.path)
                    .ok_or_else(|| not_found(&self.path))?;
                f.0.resize(len as usize, 0);
                Ok(())
            })
        }
    }

    impl tokio::io::AsyncRead for File {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let this = self.get_mut();
            let res = VFS.with_borrow(|v| {
                let (data, _) = v
                    .files
                    .get(&this.path)
                    .ok_or_else(|| not_found(&this.path))?;
                let start = (this.pos as usize).min(data.len());
                let n = buf.remaining().min(data.len() - start);
                buf.put_slice(&data[start..start + n]);
                Ok(n)
            });
            Poll::Ready(res.map(|n| this.pos += n as u64))
        }
    }

    impl tokio::io::AsyncWrite for File {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            let this = self.get_mut();
            let res = VFS.with_borrow_mut(|v| {
                let (data, modified) = v
                    .files
                    .get_mut(&this.path)
                    .ok_or_else(|| not_found(&this.path))?;
                if this.append {
                    this.pos = data.len() as u64;
                }
                let start = this.pos as usize;
                if data.len() < start + buf.len() {
                    data.resize(start + buf.len(), 0);
                }
                data[start..start + buf.len()].copy_from_slice(buf);
                *modified = now();
                Ok(buf.len())
            });
            Poll::Ready(res.inspect(|&n| this.pos += n as u64))
        }
        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_shutdown(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    impl tokio::io::AsyncSeek for File {
        fn start_seek(
            self: Pin<&mut Self>,
            position: io::SeekFrom,
        ) -> io::Result<()> {
            let this = self.get_mut();
            let len = VFS
                .with_borrow(|v| {
                    v.files.get(&this.path).map(|f| f.0.len() as i64)
                })
                .unwrap_or(0);
            let new = match position {
                io::SeekFrom::Start(n) => n as i64,
                io::SeekFrom::End(n) => len + n,
                io::SeekFrom::Current(n) => this.pos as i64 + n,
            };
            if new < 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "seek before start",
                ));
            }
            this.pos = new as u64;
            Ok(())
        }
        fn poll_complete(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<io::Result<u64>> {
            Poll::Ready(Ok(self.pos))
        }
    }
}
