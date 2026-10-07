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
}

/// Borrowed GL loader handed to mpv while the render context is created.
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
        })
    }

    /// How mpv picks the starting audio/subtitle tracks for the next file.
    /// `use_file_defaults`: honour the file's own default / forced flags (no language override).
    pub fn apply_track_prefs(&self, use_file_defaults: bool, sub_langs: &str, audio_langs: &str) {
        if use_file_defaults {
            let _ = self.mpv.set_property("alang", "");
            let _ = self.mpv.set_property("slang", "");
        } else {
            let _ = self.mpv.set_property("alang", audio_langs);
            let _ = self.mpv.set_property("slang", sub_langs);
        }
        // Newer mpv options; ignored if this libmpv doesn't know them.
        let _ = self.mpv.set_property("subs-fallback", "default");
        let _ = self.mpv.set_property("subs-fallback-forced", "always");
        // Forget a manual choice made on the previous file.
        let _ = self.mpv.set_property("aid", "auto");
        let _ = self.mpv.set_property("sid", "auto");
    }

    pub fn load(&mut self, info: PlayInfo) -> Result<()> {
        if info.start_seconds > 1.0 {
            self.mpv
                .set_property("start", format!("{:.3}", info.start_seconds).as_str())
                .map_err(|e| anyhow!("{e:?}"))?;
        } else {
            let _ = self.mpv.set_property("start", "0");
        }
        self.mpv
            .command("loadfile", &[info.url.as_str(), "replace"])
            .map_err(|e| anyhow!("loadfile failed: {e:?}"))?;
        let _ = self.mpv.set_property("pause", false);
        for (url, title, lang) in &info.external_subs {
            let _ = self
                .mpv
                .command("sub-add", &[url.as_str(), "auto", title.as_str(), lang.as_str()]);
        }
        self.current = Some(info);
        Ok(())
    }

    pub fn stop(&mut self) {
        let _ = self.mpv.command("stop", &[]);
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
    pub fn toggle_pause(&self) {
        let _ = self.mpv.set_property("pause", !self.paused());
    }
    pub fn seek_to(&self, secs: f64) {
        let _ = self
            .mpv
            .command("seek", &[format!("{secs:.2}").as_str(), "absolute"]);
    }
    pub fn seek_by(&self, secs: f64) {
        let _ = self
            .mpv
            .command("seek", &[format!("{secs:.2}").as_str(), "relative"]);
    }
    pub fn set_volume(&self, v: f64) {
        let _ = self.mpv.set_property("volume", v);
    }
    /// Audio or subtitle tracks in the currently loaded file.
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
            });
        }
        out
    }
    pub fn set_audio(&self, id: i64) {
        let _ = self.mpv.set_property("aid", id);
    }
    /// `None` turns subtitles off.
    pub fn set_sub(&self, id: Option<i64>) {
        match id {
            Some(id) => {
                let _ = self.mpv.set_property("sid", id);
            }
            None => {
                let _ = self.mpv.set_property("sid", "no");
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
}
