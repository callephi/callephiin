//! HTTP image loader that records every failed download in the debug log (status code, error,
//! unrecognised data). It is registered after egui_extras' loaders, so it handles http(s) URIs.
use eframe::egui::load::{Bytes, BytesLoadResult, BytesLoader, BytesPoll, LoadError};
use eframe::egui::Context;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type Cache = HashMap<String, Option<Result<Arc<[u8]>, String>>>; // None = still downloading

pub struct LoggingHttp {
    cache: Arc<Mutex<Cache>>,
    client: reqwest::blocking::Client,
}

impl LoggingHttp {
    pub fn new() -> Self {
        Self { cache: Default::default(), client: reqwest::blocking::Client::new() }
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
        let Ok(mut cache) = self.cache.lock() else { return Err(LoadError::Loading("image cache poisoned".into())) };
        match cache.get(uri) {
            Some(Some(Ok(bytes))) => Ok(BytesPoll::Ready { size: None, bytes: Bytes::Shared(bytes.clone()), mime: None }),
            Some(Some(Err(e))) => Err(LoadError::Loading(e.clone())),
            Some(None) => Ok(BytesPoll::Pending { size: None }),
            None => {
                cache.insert(uri.to_string(), None);
                let (uri, ctx, client, store) = (uri.to_string(), ctx.clone(), self.client.clone(), self.cache.clone());
                std::thread::spawn(move || {
                    let result = fetch(&client, &uri);
                    if let Err(e) = &result {
                        log::warn!("image {uri}: {e}");
                    }
                    if let Ok(mut c) = store.lock() {
                        c.insert(uri, Some(result));
                    }
                    ctx.request_repaint();
                });
                Ok(BytesPoll::Pending { size: None })
            }
        }
    }

    fn forget(&self, uri: &str) {
        if let Ok(mut c) = self.cache.lock() {
            c.remove(uri);
        }
    }

    fn forget_all(&self) {
        if let Ok(mut c) = self.cache.lock() {
            c.clear();
        }
    }

    fn byte_size(&self) -> usize {
        self.cache.lock().map_or(0, |c| c.values().flatten().filter_map(|r| r.as_ref().ok()).map(|b| b.len()).sum())
    }
}

fn fetch(client: &reqwest::blocking::Client, uri: &str) -> Result<Arc<[u8]>, String> {
    let resp = client.get(uri).send().map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    let bytes = resp.bytes().map_err(|e| format!("download failed: {e}"))?;
    if image::guess_format(&bytes).is_err() {
        return Err(format!("not a recognised image ({} bytes)", bytes.len()));
    }
    Ok(Arc::from(bytes.as_ref()))
}
