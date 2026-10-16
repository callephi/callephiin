//! Image handling that keeps RAM flat over long sessions.
//!
//! * Downloads go to a cache folder in callephiin's data folder instead of being held in memory. The
//!   folder is emptied when the program starts and when it exits.
//! * egui is told to drop its copies of the bytes and the decoded pixels as soon as a texture is made.
//! * Textures nobody has drawn for a while are released (they are decoded again from the cache folder
//!   if they are needed once more).
//!
//! Every failed download is recorded in the debug log (status code, error, unrecognised data).
use eframe::egui::load::{Bytes, BytesLoadResult, BytesLoader, BytesPoll, DefaultTextureLoader, LoadError, SizeHint, TextureLoadResult, TextureLoader};
use eframe::egui::{Context, TextureOptions};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub fn cache_dir() -> Option<PathBuf> {
    crate::backup::root().map(|r| r.join("cache"))
}

/// Empties (and recreates) the cache folder.
pub fn reset_cache() {
    if let Some(dir) = cache_dir() {
        let _ = std::fs::remove_dir_all(&dir);
        crate::logged!(std::fs::create_dir_all(&dir));
    }
}

/// Removes the cache folder (on exit).
pub fn clear_cache() {
    if let Some(dir) = cache_dir() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

enum State {
    Downloading,
    Ready(PathBuf),
    Failed(String),
}

pub struct LoggingHttp {
    state: Arc<Mutex<HashMap<String, State>>>,
    client: reqwest::blocking::Client,
}

impl LoggingHttp {
    pub fn new() -> Self {
        Self { state: Default::default(), client: reqwest::blocking::Client::new() }
    }

    fn start_download(&self, ctx: &Context, uri: &str) {
        let (uri, ctx, client, state) = (uri.to_string(), ctx.clone(), self.client.clone(), self.state.clone());
        std::thread::spawn(move || {
            let result = fetch(&client, &uri);
            if let Err(e) = &result {
                log::warn!("image {uri}: {e}");
            }
            if let Ok(mut s) = state.lock() {
                s.insert(uri, result.map_or_else(State::Failed, State::Ready));
            }
            ctx.request_repaint();
        });
    }
}

impl BytesLoader for LoggingHttp {
    fn id(&self) -> &str {
        "callephiin::LoggingHttp"
    }

    fn load(&self, ctx: &Context, uri: &str) -> BytesLoadResult {
        if !(uri.starts_with("http://") || uri.starts_with("https://")) {
            return Err(LoadError::NotSupported);
        }
        let Ok(mut state) = self.state.lock() else { return Err(LoadError::Loading("image cache poisoned".into())) };
        match state.get(uri) {
            Some(State::Ready(path)) => match std::fs::read(path) {
                Ok(bytes) => return Ok(BytesPoll::Ready { size: None, bytes: Bytes::Shared(Arc::from(bytes)), mime: None }),
                // the file went missing (cache folder cleared): fetch it again
                Err(_) => {
                    state.insert(uri.to_string(), State::Downloading);
                }
            },
            Some(State::Failed(e)) => return Err(LoadError::Loading(e.clone())),
            Some(State::Downloading) => return Ok(BytesPoll::Pending { size: None }),
            None => {
                state.insert(uri.to_string(), State::Downloading);
            }
        }
        drop(state);
        self.start_download(ctx, uri);
        Ok(BytesPoll::Pending { size: None })
    }

    // The files stay on disk, and the table only holds their paths, so there is nothing worth freeing
    // except failures, which get another try.
    fn forget(&self, uri: &str) {
        if let Ok(mut s) = self.state.lock() {
            if matches!(s.get(uri), Some(State::Failed(_))) {
                s.remove(uri);
            }
        }
    }

    fn forget_all(&self) {
        if let Ok(mut s) = self.state.lock() {
            s.retain(|_, v| !matches!(v, State::Failed(_)));
        }
    }

    fn byte_size(&self) -> usize {
        0
    }
}

fn fetch(client: &reqwest::blocking::Client, uri: &str) -> Result<PathBuf, String> {
    let resp = client.get(uri).send().map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    let bytes = resp.bytes().map_err(|e| format!("download failed: {e}"))?;
    if image::guess_format(&bytes).is_err() {
        return Err(format!("not a recognised image ({} bytes)", bytes.len()));
    }
    let dir = cache_dir().ok_or("no cache folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("cache folder: {e}"))?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    uri.hash(&mut h);
    let path = dir.join(format!("{:016x}.img", h.finish()));
    std::fs::write(&path, &bytes).map_err(|e| format!("cache write: {e}"))?;
    Ok(path)
}

/// egui's own texture cache, plus a note of when each image was last drawn so idle ones can be released.
pub struct TrackingTextures {
    inner: DefaultTextureLoader,
    last_used: Mutex<HashMap<String, Instant>>,
}

/// Images idle for this long are released.
const IDLE: Duration = Duration::from_secs(20);
/// Above this many live images, the least recently drawn go first (never ones drawn in the last 3 s).
const MAX_LIVE: usize = 120;

impl TrackingTextures {
    pub fn new() -> Self {
        Self { inner: DefaultTextureLoader::default(), last_used: Default::default() }
    }

    /// Releases textures that are no longer being drawn. Cheap; call every few seconds.
    pub fn sweep(&self, ctx: &Context) {
        let now = Instant::now();
        let stale: Vec<String> = {
            let Ok(mut map) = self.last_used.lock() else { return };
            let mut by_age: Vec<(String, Duration)> = map.iter().map(|(k, t)| (k.clone(), now.duration_since(*t))).collect();
            by_age.sort_by(|a, b| b.1.cmp(&a.1));
            let over = map.len().saturating_sub(MAX_LIVE);
            let stale: Vec<String> = by_age
                .into_iter()
                .enumerate()
                .filter(|(i, (_, age))| *age > IDLE || (*i < over && *age > Duration::from_secs(3)))
                .map(|(_, (k, _))| k)
                .collect();
            for k in &stale {
                map.remove(k);
            }
            stale
        };
        for uri in stale {
            ctx.forget_image(&uri);
        }
    }
}

impl TextureLoader for TrackingTextures {
    fn id(&self) -> &str {
        "callephiin::TrackingTextures"
    }

    fn load(&self, ctx: &Context, uri: &str, texture_options: TextureOptions, size_hint: SizeHint) -> TextureLoadResult {
        if let Ok(mut map) = self.last_used.lock() {
            match map.get_mut(uri) {
                Some(t) => *t = Instant::now(),
                None => {
                    map.insert(uri.to_string(), Instant::now());
                }
            }
        }
        self.inner.load(ctx, uri, texture_options, size_hint)
    }

    fn forget(&self, uri: &str) {
        self.inner.forget(uri);
    }

    fn forget_all(&self) {
        self.inner.forget_all();
        if let Ok(mut map) = self.last_used.lock() {
            map.clear();
        }
    }

    fn end_pass(&self, frame_index: usize) {
        self.inner.end_pass(frame_index);
    }

    fn byte_size(&self) -> usize {
        self.inner.byte_size()
    }
}
