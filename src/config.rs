use crate::debuglog::LogOk;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Decides between Direct Play and transcoding
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamMode {
    /// Direct Play on local/private networks, transcode (bitrate-capped) otherwise
    Auto,
    AlwaysDirect,
    AlwaysTranscode,
}

impl Default for StreamMode {
    fn default() -> Self {
        StreamMode::Auto
    }
}

/// A track described by its names (not its index, which differs between files)
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StoredTrack {
    pub lang: String,
    pub title: String,
    pub codec: String,
    pub forced: bool,
}

/// The audio/subtitle choice the user made for one show
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SeriesTracks {
    pub audio: Option<StoredTrack>,
    pub sub: Option<StoredTrack>,
    /// The user turned subtitles off
    pub sub_off: bool,
}

/// A saved sign-in: server + user (the access token is stored, never the password)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub id: String,
    /// Optional custom display name
    pub name: String,
    pub server_url: String,
    pub username: String,
    pub user_id: String,
    pub access_token: String,
    /// Device id this profile signed in with. Jellyfin keeps one session per user and device, so
    /// profiles for the same user (e.g. reached through two domains) need devices of their own,
    /// or signing in to one logs the other out. Empty = an older profile that uses the shared id.
    pub device_id: String,
}

impl Profile {
    pub fn display_name(&self) -> String {
        if self.name.trim().is_empty() { self.username.clone() } else { self.name.clone() }
    }
    pub fn host(&self) -> String {
        self.server_url.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_string()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server_url: String,
    pub username: String,
    pub user_id: String,
    pub access_token: String,
    pub device_id: String,
    /// Set while signing in: the device id to sign in with (becomes the profile's)
    #[serde(skip)]
    pub login_device_id: Option<String>,

    pub stream_mode: StreamMode,
    /// Remote streaming cap in kbit/s (used when transcoding)
    pub remote_bitrate_kbps: u32,

    pub discord_enabled: bool,

    /// Start every file with its own default / forced audio and subtitle tracks
    /// When off, `preferred_*_lang` below decide instead
    pub use_file_default_tracks: bool,
    /// Keep the audio/subtitle choice made for a show for the rest of that show
    pub remember_tracks: bool,
    /// series id -> remembered tracks
    pub series_tracks: HashMap<String, SeriesTracks>,
    pub preferred_sub_lang: String,
    pub preferred_audio_lang: String,
    pub sub_font_size: u32,
    pub volume: f64,
    /// Content (home/settings) scale. 0.0 = automatic from monitor size
    pub ui_scale: f32,
    /// The user picked "Custom" in the scale list (so a custom value equal to a preset stays custom)
    pub ui_scale_custom: bool,
    /// Unused (sidecar loading is no longer a setting); kept so old config files still load
    pub external_tracks: bool,
    /// Seconds jumped by the back/forward buttons or arrow keys
    pub skip_back_secs: u32,
    pub skip_fwd_secs: u32,
    /// Items per page when browsing a library
    #[serde(default = "default_page_size")]
    pub library_page_size: usize,
    /// Hide titles, thumbnails and synopses of unwatched episodes
    pub spoiler_control: bool,
    pub profiles: Vec<Profile>,
    /// ID of the profile that is signed in or used last
    pub last_profile: String,
    /// Toggle for showing profile switcher on launch
    pub switcher_on_launch: bool,
    /// Write a debug log file (Settings → Diagnostics)
    pub debug_log: bool,
    /// V-Sync: a first-run config turns it on only when the ANGLE libraries are present (the native NVIDIA OpenGL path flickers with it on).
    /// `skip_mpv` is a diagnostic: do not create the mpv player at all.
    pub vsync: bool,
    pub skip_mpv: bool,
    /// Wait for the GPU to finish every frame (avoids flashing / tearing on some setups).
    pub gl_finish: bool,
    /// Draw through ANGLE (OpenGL ES on Direct3D 11) on Windows when libEGL.dll / libGLESv2.dll sit next
    /// to the program; falls back to the plain OpenGL driver otherwise. Needs a restart.
    pub angle: bool,
    /// Windows: wait for the desktop compositor after each frame (stops tearing bars with vsync off)
    pub dwm_flush: bool,
    /// Sonarr address and API key (optional)
    pub sonarr_url: String,
    pub sonarr_key: String,
    /// Per profile and title: which version was watched last and how far each version got
    pub versions: HashMap<String, VersionMem>,
}

/// Jellyfin keeps one resume point per title, so the app remembers the rest itself.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VersionMem {
    /// Media source id of the version watched last (what Continue Watching resumes)
    pub last: String,
    /// Seconds watched per media source id; -1 = watched
    pub pos: HashMap<String, f64>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            username: String::new(),
            user_id: String::new(),
            access_token: String::new(),
            device_id: uuid::Uuid::new_v4().to_string(),
            login_device_id: None,
            stream_mode: StreamMode::Auto,
            remote_bitrate_kbps: 8_000,
            discord_enabled: true,
            use_file_default_tracks: true,
            remember_tracks: true,
            series_tracks: HashMap::new(),
            preferred_sub_lang: "eng".into(),
            preferred_audio_lang: "eng,jpn".into(),
            sub_font_size: 44,
            volume: 100.0,
            ui_scale: 0.0,
            ui_scale_custom: false,
            external_tracks: true,
            skip_back_secs: 10,
            skip_fwd_secs: 30,
            library_page_size: default_page_size(),
            spoiler_control: false,
            profiles: Vec::new(),
            last_profile: String::new(),
            switcher_on_launch: false,
            debug_log: false,
            // V-Sync only flickers on the native NVIDIA OpenGL path, so a first-run config turns it on
            // exactly when the ANGLE libraries are next to the program
            vsync: cfg!(target_os = "macos") || angle_libs_present(),
            skip_mpv: false,
            gl_finish: true,
            angle: true,
            dwm_flush: true,
            sonarr_url: String::new(),
            sonarr_key: String::new(),
            versions: HashMap::new(),
        }
    }
}

impl Config {
    fn vkey(&self, item: &str) -> String {
        format!("{}:{item}", self.user_id)
    }

    pub fn version_mem(&self, item: &str) -> VersionMem {
        self.versions.get(&self.vkey(item)).cloned().unwrap_or_default()
    }

    /// Playback of `source` stopped at `pos` seconds (of `dur`)
    pub fn remember_stop(&mut self, item: &str, source: &str, pos: f64, dur: f64) {
        let k = self.vkey(item);
        let m = self.versions.entry(k).or_default();
        m.last = source.to_string();
        if dur > 0.0 && pos > 0.92 * dur {
            m.pos.insert(source.to_string(), -1.0);
        } else if pos > 1.0 {
            m.pos.insert(source.to_string(), pos);
        } else {
            m.pos.remove(source);
        }
    }

    /// Marks one version watched / unwatched; returns whether any other version is still watched
    pub fn mark_version(&mut self, item: &str, source: &str, watched: bool) -> bool {
        let k = self.vkey(item);
        let m = self.versions.entry(k).or_default();
        m.last = source.to_string();
        if watched {
            m.pos.insert(source.to_string(), -1.0);
        } else {
            m.pos.remove(source);
        }
        m.pos.iter().any(|(s, v)| s != source && *v < 0.0)
    }

    /// Marks every version of a title watched, or forgets all version progress
    pub fn set_all_versions(&mut self, item: &str, sources: &[String], watched: bool) {
        let k = self.vkey(item);
        if watched {
            let m = self.versions.entry(k).or_default();
            for s in sources {
                m.pos.insert(s.clone(), -1.0);
            }
        } else {
            self.versions.remove(&k);
        }
    }

    pub fn path() -> Option<PathBuf> {
        // <app folder>/config/config.json on every platform, so an export from one OS imports on another
        crate::backup::root().map(|r| r.join("config").join("config.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| fs::read_to_string(p).log_ok())
            .and_then(|s| serde_json::from_str(&s).log_ok())
            .map(|mut c: Config| {
                c.migrate_profiles();
                c
            })
            .unwrap_or_default()
    }

    /// Configs from before profiles existed: turns the signed-in account into the first profile.
	/// This probably ain't needed. The damn thing was private before v1.0.0, but in case someone
	/// decides they should use the pre-release that IS publically available, well...
	/// That's your own fault, man.
    fn migrate_profiles(&mut self) {
        if self.profiles.is_empty() && self.logged_in() {
            self.upsert_profile("");
        }
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    /// Save the currently signed-in account as a profile (updating the one for the same
    /// server + user) and make it the last used one
    pub fn upsert_profile(&mut self, name: &str) {
        let norm = |u: &str| u.trim().trim_end_matches('/').to_lowercase();
        let existing = self
            .profiles
            .iter()
            .position(|p| norm(&p.server_url) == norm(&self.server_url) && p.username.eq_ignore_ascii_case(&self.username));
        let idx = match existing {
            Some(i) => i,
            None => {
                self.profiles.push(Profile { id: uuid::Uuid::new_v4().to_string(), ..Default::default() });
                self.profiles.len() - 1
            }
        };
        let p = &mut self.profiles[idx];
        p.server_url = self.server_url.clone();
        p.username = self.username.clone();
        p.user_id = self.user_id.clone();
        p.access_token = self.access_token.clone();
        if let Some(d) = self.login_device_id.take() {
            p.device_id = d;
        }
        if !name.trim().is_empty() {
            p.name = name.trim().to_string();
        }
        self.last_profile = p.id.clone();
    }

    /// Device id to present to the server for the active profile
    pub fn active_device_id(&self) -> String {
        if let Some(d) = &self.login_device_id {
            return d.clone();
        }
        self.profile(&self.last_profile).map(|p| p.device_id.clone()).filter(|d| !d.is_empty()).unwrap_or_else(|| self.device_id.clone())
    }

    /// Device id for signing in as `server` + `user`: the saved profile's own, else a fresh one
    pub fn device_id_for_login(&self, server: &str, user: &str) -> String {
        let norm = |u: &str| u.trim().trim_end_matches('/').to_lowercase();
        self.profiles
            .iter()
            .find(|p| norm(&p.server_url) == norm(server) && p.username.eq_ignore_ascii_case(user))
            .map(|p| p.device_id.clone())
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
    }

    /// Make a saved profile the signed-in account
    pub fn activate(&mut self, id: &str) -> bool {
        let Some(p) = self.profile(id).cloned() else { return false };
        self.server_url = p.server_url;
        self.username = p.username;
        self.user_id = p.user_id;
        self.access_token = p.access_token;
        self.last_profile = p.id;
        true
    }

    /// Forget the access token of the active account (profile stays, sign in needed again)
    pub fn sign_out(&mut self) {
        let id = self.last_profile.clone();
        if let Some(p) = self.profiles.iter_mut().find(|p| p.id == id) {
            p.access_token.clear();
        }
        self.access_token.clear();
        self.user_id.clear();
    }

    pub fn save(&self) {
        if let Some(p) = Self::path() {
            if let Some(dir) = p.parent() {
                crate::logged!(fs::create_dir_all(dir));
            }
            if let Ok(s) = serde_json::to_string_pretty(self) {
                crate::logged!(fs::write(p, s));
            }
        }
    }

    pub fn logged_in(&self) -> bool {
        !self.access_token.is_empty() && !self.server_url.is_empty()
    }
}

fn default_page_size() -> usize {
    25
}

/// True when all three ANGLE libraries sit in the program's folder.
pub fn angle_libs_present() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| ["libEGL.dll", "libGLESv2.dll", "d3dcompiler_47.dll"].iter().all(|f| d.join(f).is_file())))
        .unwrap_or(false)
}
