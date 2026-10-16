use crate::config::Config;
use crate::jellyfin::PlayInfo;
use anyhow::{anyhow, Result};
use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use libmpv2::Mpv;
use std::ffi::{c_void, CStr, CString};
use std::sync::{Arc, Mutex};

/// Wrapper so the render context can live inside the egui paint callback
/// (which must be `Send + Sync`). It is only ever touched on the GL thread.
pub struct GlRender(pub RenderContext);
unsafe impl Send for GlRender {}
unsafe impl Sync for GlRender {}

pub struct Player {
    // Field order matters: the render context must drop before the mpv handle.
    pub render: Arc<Mutex<GlRender>>,
    pub mpv: Mpv,
    pub current: Option<PlayInfo>,
    /// server-provided subtitle files, added once the file has actually opened
    pending_subs: std::cell::RefCell<Vec<(String, String, String)>>,
}

/// Borrowed GL loader handed to mpv while the render context is created
struct Loader<'a>(&'a dyn Fn(&CStr) -> *const c_void);

fn get_proc_address(loader: &Loader<'_>, name: &str) -> *mut c_void {
    match CString::new(name) {
        Ok(c) => (loader.0)(&c) as *mut c_void,
        Err(_) => std::ptr::null_mut(),
    }
}

impl Player {
    pub fn new(
        cfg: &Config,
        gl_loader: &dyn Fn(&CStr) -> *const c_void,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self> {
        let sub_size = cfg.sub_font_size.to_string();
        let mut mpv = Mpv::with_initializer(|init| {
            // --- rendering / decoding -------------------------------------------------
            init.set_property("vo", "libmpv")?;
            // GPU decoding for H.264 / HEVC / AV1 (falls back to software, e.g. dav1d).
            init.set_property("hwdec", "auto-safe")?;
            init.set_property("keep-open", "yes")?;
            init.set_property("cache", "yes")?;
            init.set_property("demuxer-max-bytes", "200MiB")?;
            init.set_property("demuxer-max-back-bytes", "50MiB")?;
            init.set_property("video-sync", "audio")?;
            init.set_property("interpolation", "no")?;

            // --- HDR -> SDR tone mapping ---------------------------------------------
            // The GL framebuffer is an SDR window, so HDR10/HLG/Dolby Vision (base layer)
            // content is tone mapped with BT.2390 and per-scene peak detection.
            init.set_property("tone-mapping", "bt.2390")?;
            init.set_property("hdr-compute-peak", "yes")?;
            init.set_property("target-colorspace-hint", "no")?;
            init.set_property("gamut-mapping-mode", "auto")?;

            // --- subtitles -------------------------------------------------------------
            // Text subs (SRT/VTT/...) get a VLC-like look: white sans text, black outline.
            // Styled ASS/SSA (anime etc.) keeps the script's own fonts/styles because
            // ass-override stays at "scale"; embedded fonts are loaded from the file.
            init.set_property("sub-auto", "no")?;
            init.set_property("embeddedfonts", "yes")?;
            init.set_property("sub-ass-override", "scale")?;
            init.set_property("sub-font", "Arial")?;
            init.set_property("sub-font-size", sub_size.as_str())?;
            init.set_property("sub-color", "#FFFFFF")?;
            init.set_property("sub-border-color", "#000000")?;
            init.set_property("sub-border-size", "2.4")?;
            init.set_property("sub-shadow-offset", "0")?;
            init.set_property("sub-bold", "no")?;
            init.set_property("sub-margin-y", "36")?;
            // Track language preferences are applied per file in `apply_track_prefs`.

            if cfg.debug_log {
                if let Some(p) = crate::debuglog::mpv_path() {
                    init.set_property("log-file", p.to_string_lossy().as_ref())?;
                }
            }
            init.set_property("volume", cfg.volume)?;
            // The client draws its own UI.
            init.set_property("osc", "no")?;
            init.set_property("osd-level", "0")?;
            init.set_property("osd-bar", "no")?;
            init.set_property("input-default-bindings", "no")?;
            init.set_property("input-vo-keyboard", "no")?;
            Ok(())
        })
        .map_err(|e| anyhow!("failed to initialise libmpv: {e:?}"))?;

        let mut rc = RenderContext::new(
            unsafe { mpv.ctx.as_mut() },
            vec![
                RenderParam::ApiType(RenderParamApiType::OpenGl),
                RenderParam::InitParams(OpenGLInitParams {
                    get_proc_address,
                    ctx: Loader(gl_loader),
                }),
            ],
        )
        .map_err(|e| anyhow!("failed to create mpv render context: {e:?}"))?;
        rc.set_update_callback(repaint);

        Ok(Self {
            render: Arc::new(Mutex::new(GlRender(rc))),
            mpv,
            current: None,
            pending_subs: Default::default(),
        })
    }

    /// How mpv picks the starting audio/subtitle tracks for the next file.
    /// `use_file_defaults`: honour the file's own default / forced flags (no language override)
    pub fn apply_track_prefs(&self, use_file_defaults: bool, sub_langs: &str, audio_langs: &str) {
        if use_file_defaults {
            crate::logged!(self.mpv.set_property("alang", ""));
            crate::logged!(self.mpv.set_property("slang", ""));
        } else {
            crate::logged!(self.mpv.set_property("alang", audio_langs));
            crate::logged!(self.mpv.set_property("slang", sub_langs));
        }
        // Newer mpv options; ignored if this libmpv doesn't know them.
        crate::logged!(self.mpv.set_property("subs-fallback", "default"));
        crate::logged!(self.mpv.set_property("subs-fallback-forced", "always"));
        // Forget a manual choice made on the previous file.
        crate::logged!(self.mpv.set_property("aid", "auto"));
        crate::logged!(self.mpv.set_property("sid", "auto"));
    }

    pub fn load(&mut self, info: PlayInfo) -> Result<()> {
        log::debug!("play {} (transcoding: {}, start {:.1}s)", info.item.id, info.transcoding, info.start_seconds);
        if info.start_seconds > 1.0 {
            self.mpv
                .set_property("start", format!("{:.3}", info.start_seconds).as_str())
                .map_err(|e| anyhow!("{e:?}"))?;
        } else {
            crate::logged!(self.mpv.set_property("start", "0"));
        }
        self.mpv
            .command("loadfile", &[info.url.as_str(), "replace"])
            .map_err(|e| anyhow!("loadfile failed: {e:?}"))?;
        // Start paused: the app applies remembered tracks first, then un-pauses, so the
        // audio / subtitle reinit happens before the picture starts moving.
        crate::logged!(self.mpv.set_property("pause", true));
        // sub-add straight after loadfile runs before the file is open and is dropped, so queue them
        *self.pending_subs.borrow_mut() = info
            .external_subs
            .iter()
            .map(|(u, t, l)| (u.clone(), t.clone(), l.clone()))
            .collect();
        self.current = Some(info);
        Ok(())
    }

    /// Adds the queued subtitle files once the file is open; true when nothing is left to add.
    /// Only url, flags and title are passed: some mpv builds reject a 4th (lang) argument.
    pub fn flush_subs(&self) -> bool {
        if self.pending_subs.borrow().is_empty() {
            return true;
        }
        if self.mpv.get_property::<i64>("track-list/count").unwrap_or(0) == 0 {
            return false;
        }
        for (url, title, lang) in std::mem::take(&mut *self.pending_subs.borrow_mut()) {
            // libmpv2's command() joins the arguments with spaces and parses the string, so a
            // title like "English - Default - ASS" must be quoted or it splits into extra args
            let q = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
            crate::logged!(self.mpv.command("sub-add", &[q(&url).as_str(), "auto", q(&title).as_str(), q(&lang).as_str()]));
        }
        true
    }

    pub fn play_audio_loop(&mut self, path: &str) {
        crate::logged!(self.mpv.set_property("loop-file", "inf"));
        crate::logged!(self.mpv.set_property("start", "0"));
        let q = format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""));
        crate::logged!(self.mpv.command("loadfile", &[q.as_str(), "replace"]));
        crate::logged!(self.mpv.set_property("pause", false));
    }

    pub fn stop(&mut self) {
        crate::logged!(self.mpv.set_property("loop-file", "no"));
        crate::logged!(self.mpv.command("stop", &[]));
        self.current = None;
    }

    pub fn position(&self) -> f64 {
        self.mpv.get_property::<f64>("time-pos").unwrap_or(0.0)
    }
    pub fn duration(&self) -> f64 {
        self.mpv.get_property::<f64>("duration").unwrap_or(0.0)
    }
    pub fn paused(&self) -> bool {
        self.mpv.get_property::<bool>("pause").unwrap_or(false)
    }
    pub fn ended(&self) -> bool {
        self.mpv.get_property::<bool>("eof-reached").unwrap_or(false)
    }
    pub fn volume(&self) -> f64 {
        self.mpv.get_property::<f64>("volume").unwrap_or(100.0)
    }
    pub fn muted(&self) -> bool {
        self.mpv.get_property::<bool>("mute").unwrap_or(false)
    }
    pub fn set_muted(&self, m: bool) {
        crate::logged!(self.mpv.set_property("mute", m));
    }
    pub fn toggle_pause(&self) {
        crate::logged!(self.mpv.set_property("pause", !self.paused()));
    }
    pub fn seek_to(&self, secs: f64) {
        crate::logged!(self
            .mpv
            .command("seek", &[format!("{secs:.2}").as_str(), "absolute+exact"]));
    }
    pub fn seek_by(&self, secs: f64) {
        crate::logged!(self
            .mpv
            .command("seek", &[format!("{secs:.2}").as_str(), "relative"]));
    }
    pub fn set_volume(&self, v: f64) {
        crate::logged!(self.mpv.set_property("volume", v));
    }
    /// Audio or subtitle tracks in the currently loaded file
    pub fn tracks(&self, kind: TrackKind) -> Vec<Track> {
        let want = match kind {
            TrackKind::Audio => "audio",
            TrackKind::Sub => "sub",
        };
        let n = self.mpv.get_property::<i64>("track-list/count").unwrap_or(0);
        let mut out = Vec::new();
        for i in 0..n {
            let get = |k: &str| self.mpv.get_property::<String>(&format!("track-list/{i}/{k}")).unwrap_or_default();
            let flag = |k: &str| self.mpv.get_property::<bool>(&format!("track-list/{i}/{k}")).unwrap_or(false);
            if get("type") != want {
                continue;
            }
            out.push(Track {
                id: self.mpv.get_property::<i64>(&format!("track-list/{i}/id")).unwrap_or(0),
                lang: get("lang"),
                title: get("title"),
                codec: get("codec"),
                selected: flag("selected"),
                forced: flag("forced"),
                default: flag("default"),
            });
        }
        out
    }
    pub fn set_audio(&self, id: i64) {
        crate::logged!(self.mpv.set_property("aid", id));
    }
    /// `None` turns subtitles off, obviously
    pub fn set_sub(&self, id: Option<i64>) {
        match id {
            Some(id) => {
                crate::logged!(self.mpv.set_property("sid", id));
            }
            None => {
                crate::logged!(self.mpv.set_property("sid", "no"));
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrackKind {
    Audio,
    Sub,
}

#[derive(Clone, Debug)]
pub struct Track {
    pub id: i64,
    pub lang: String,
    pub title: String,
    pub codec: String,
    pub selected: bool,
    pub forced: bool,
    pub default: bool,
}
