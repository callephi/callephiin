use directories::ProjectDirs;
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
    /// Load external (sidecar) subtitle files the server lists for a title. Why doesn't this work
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            username: String::new(),
            user_id: String::new(),
            access_token: String::new(),
            device_id: uuid::Uuid::new_v4().to_string(),
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
        }
    }
}

impl Config {
    fn path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "callephiin").map(|d| d.config_dir().join("config.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
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
        if !name.trim().is_empty() {
            p.name = name.trim().to_string();
        }
        self.last_profile = p.id.clone();
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
                let _ = fs::create_dir_all(dir);
            }
            if let Ok(s) = serde_json::to_string_pretty(self) {
                let _ = fs::write(p, s);
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
