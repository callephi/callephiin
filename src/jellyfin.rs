use crate::config::{Config, StreamMode};
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::IpAddr;

const CLIENT_NAME: &str = "callephiin";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEVICE_NAME: &str = "Windows";

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Item {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "Type", default)]
    pub kind: String,
    pub production_year: Option<u32>,
    pub official_rating: Option<String>,
    pub community_rating: Option<f32>,
    pub run_time_ticks: Option<i64>,
    #[serde(default)]
    pub genres: Vec<String>,
    pub overview: Option<String>,
    pub series_name: Option<String>,
    pub series_id: Option<String>,
    /// For library folders: "movies", "tvshows", ...
    pub collection_type: Option<String>,
    /// For series: total episode count
    pub recursive_item_count: Option<u32>,
    pub season_id: Option<String>,
    pub parent_index_number: Option<u32>,
    pub index_number: Option<u32>,
    #[serde(default)]
    pub image_tags: HashMap<String, String>,
    #[serde(default)]
    pub backdrop_image_tags: Vec<String>,
    /// Present on episodes when the parent series has a title logo
    pub parent_logo_image_tag: Option<String>,
    #[serde(default)]
    pub parent_backdrop_image_tags: Vec<String>,
    #[serde(default)]
    pub user_data: Option<UserData>,
    /// Cast and crew (only returned by the single-item endpoint)
    #[serde(default)]
    pub people: Vec<Person>,
    pub original_title: Option<String>,
    /// ISO timestamp, e.g. "2026-10-05T00:00:00.0000000Z"
    pub premiere_date: Option<String>,
    /// For seasons: number of episodes
    pub child_count: Option<u32>,
    /// Specials placed in the watch order (NFO displayseason / displayepisode); -1 or absent = unplaced
    pub airs_before_season_number: Option<i32>,
    pub airs_before_episode_number: Option<i32>,
    #[serde(default)]
    pub studios: Vec<Named>,
    #[serde(default)]
    pub production_locations: Vec<String>,
    /// Client-side tag for extras: (owning show/movie id, owner has a logo)
    #[serde(skip)]
    pub owner: Option<(String, bool)>,
    /// Client-side tag on a person's page: the first role they play in this title
    #[serde(skip)]
    pub role: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Named {
    #[serde(default)]
    pub name: String,
}

/// An actor / crew member, either from an item's cast list or from a people search.
#[derive(Clone, Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Person {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub role: Option<String>,
    #[serde(rename = "Type", default)]
    pub kind: String,
    pub primary_image_tag: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SearchResults {
    pub titles: Vec<Item>,
    pub episodes: Vec<Item>,
    pub people: Vec<Person>,
}

/// Everything the title page needs for the first paint
#[derive(Clone, Debug)]
pub struct TitleData {
    pub item: Item,
    /// What "Play" starts for a show (next unwatched episode)
    pub next_ep: Option<Item>,
    pub similar: Vec<Item>,
    pub seasons: Vec<Item>,
    /// Season whose episodes are in `episodes` (None for movies)
    pub season_id: Option<String>,
    pub episodes: Vec<Item>,
    pub series_extras: Vec<Item>,
    pub season_extras: Vec<Item>,
    /// Episode to line the episode row up with (last one watched)
    pub focus: Option<String>,
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct UserData {
    #[serde(default)]
    pub playback_position_ticks: i64,
    #[serde(default)]
    pub played: bool,
    #[serde(default)]
    pub is_favorite: bool,
}

impl Item {
    pub fn runtime_minutes(&self) -> Option<i64> {
        self.run_time_ticks.map(|t| t / 10_000_000 / 60)
    }

    pub fn is_episode(&self) -> bool {
        self.kind == "Episode"
    }

    /// "S1:E3" style code, if the item has season/episode numbers
    pub fn ep_code(&self) -> Option<String> {
        match (self.parent_index_number, self.index_number) {
            (Some(s), Some(e)) => Some(format!("S{s}:E{e}")),
            (None, Some(e)) => Some(format!("E{e}")),
            _ => None,
        }
    }

    /// "S1:E3 · Episode Title" for episodes, plain title otherwise
    pub fn episode_label(&self) -> String {
        match self.ep_code() {
            Some(c) if self.name.is_empty() => c,
            Some(c) => format!("{c} · {}", self.name),
            None => self.name.clone(),
        }
    }

    /// Fraction watched (0..1) from the user's saved position
    pub fn progress(&self) -> f32 {
        let total = self.run_time_ticks.unwrap_or(0) as f64;
        let pos = self.user_data.as_ref().map(|u| u.playback_position_ticks as f64).unwrap_or(0.0);
        if total > 0.0 { (pos / total).clamp(0.0, 1.0) as f32 } else { 0.0 }
    }

    /// Seconds left according to the saved position
    pub fn remaining_seconds(&self) -> Option<f64> {
        let total = self.run_time_ticks? as f64 / 10_000_000.0;
        Some((total - self.resume_seconds()).max(0.0))
    }

    /// Title shown as the main line (series name for episodes)
    pub fn display_title(&self) -> String {
        if self.is_episode() {
            self.series_name.clone().unwrap_or_else(|| self.name.clone())
        } else {
            self.name.clone()
        }
    }

    /// The item whose artwork represents the show (series for episodes)
    pub fn art_id(&self) -> &str {
        if let Some((o, _)) = &self.owner {
            o
        } else if self.is_episode() {
            self.series_id.as_deref().unwrap_or(&self.id)
        } else {
            &self.id
        }
    }

    /// Minimal series item for opening a show page from one of its episodes
    pub fn series_stub(&self) -> Item {
        Item {
            id: self.art_id().to_string(),
            name: self.series_name.clone().unwrap_or_else(|| self.name.clone()),
            kind: "Series".into(),
            ..Default::default()
        }
    }

    /// Native / original title when it differs from the displayed one
    pub fn native_title(&self) -> Option<&str> {
        self.original_title.as_deref().filter(|t| !t.is_empty() && *t != self.name)
    }

    pub fn played(&self) -> bool {
        self.user_data.as_ref().map_or(false, |u| u.played)
    }

    pub fn resume_seconds(&self) -> f64 {
        self.user_data
            .as_ref()
            .map(|u| u.playback_position_ticks as f64 / 10_000_000.0)
            .unwrap_or(0.0)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ItemsResponse {
    items: Vec<Item>,
    #[serde(default)]
    total_record_count: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AuthResponse {
    access_token: String,
    user: AuthUser,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AuthUser {
    id: String,
}

/// Everything the player needs to start a stream
#[derive(Clone, Debug)]
pub struct PlayInfo {
    pub item: Item,
    pub url: String,
    pub play_session_id: String,
    pub media_source_id: String,
    pub transcoding: bool,
    pub start_seconds: f64,
    /// (url, title, language) of external subtitle tracks to load into mpv.
    pub external_subs: Vec<(String, String, String)>,
}

/// Trickplay sheet layout for one video
#[derive(Clone, Debug)]
pub struct Trick {
    pub item_id: String,
    pub media_source_id: String,
    pub width: u32,
    pub thumb_w: u32,
    pub thumb_h: u32,
    pub tile_w: u32,
    pub tile_h: u32,
    pub count: u32,
    pub interval_ms: u64,
}

impl Trick {
    /// (sheet index, column, row) holding the thumbnail for time `t` seconds
    pub fn locate(&self, t: f64) -> (u32, u32, u32) {
        let n = ((t * 1000.0 / self.interval_ms as f64) as u32).min(self.count.saturating_sub(1));
        let per = self.tile_w * self.tile_h;
        (n / per, n % per % self.tile_w, n % per / self.tile_w)
    }
}

#[derive(Clone)]
pub struct Client {
    pub base: String,
    pub token: String,
    pub user_id: String,
    pub device_id: String,
    http: reqwest::blocking::Client,
}

impl Client {
    pub fn from_config(cfg: &Config) -> Self {
        Self {
            base: cfg.server_url.trim_end_matches('/').to_string(),
            token: cfg.access_token.clone(),
            user_id: cfg.user_id.clone(),
            device_id: cfg.device_id.clone(),
            http: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("http client"),
        }
    }

    fn auth_header(&self) -> String {
        format!(
            "MediaBrowser Client=\"{CLIENT_NAME}\", Device=\"{DEVICE_NAME}\", DeviceId=\"{}\", Version=\"{CLIENT_VERSION}\", Token=\"{}\"",
            self.device_id, self.token
        )
    }

    /// Log in with username/password and return an updated config
    pub fn authenticate(cfg: &Config, password: &str) -> Result<Config> {
        let c = Self::from_config(cfg);
        let resp: AuthResponse = c
            .http
            .post(format!("{}/Users/AuthenticateByName", c.base))
            .header("Authorization", c.auth_header())
            .json(&json!({ "Username": cfg.username, "Pw": password }))
            .send()?
            .error_for_status()
            .context("login failed (check URL, username and password)")?
            .json()?;
        let mut out = cfg.clone();
        out.access_token = resp.access_token;
        out.user_id = resp.user.id;
        out.server_url = c.base;
        Ok(out)
    }

    fn get(&self, path: &str) -> Result<reqwest::blocking::Response> {
        Ok(self
            .http
            .get(format!("{}{}", self.base, path))
            .header("Authorization", self.auth_header())
            .send()?
            .error_for_status()?)
    }

    fn post(&self, path: &str, body: &Value) -> Result<reqwest::blocking::Response> {
        Ok(self
            .http
            .post(format!("{}{}", self.base, path))
            .header("Authorization", self.auth_header())
            .json(body)
            .send()?
            .error_for_status()?)
    }

    const FIELDS: &'static str = "Overview,Genres,CommunityRating,OfficialRating,RunTimeTicks,ProductionYear,OriginalTitle";

    /// Latest media for the home screen
    pub fn home_items(&self, limit: u32) -> Result<Vec<Item>> {
        let path = format!(
            "/Users/{}/Items?Recursive=true&IncludeItemTypes=Movie,Series&SortBy=DateCreated&SortOrder=Descending&Limit={limit}&Fields={}&ImageTypeLimit=1&EnableImageTypes=Primary,Backdrop,Logo",
            self.user_id,
            Self::FIELDS
        );
        Ok(self.get(&path)?.json::<ItemsResponse>()?.items)
    }

    /// Items the user can resume
    pub fn resume_items(&self, limit: u32) -> Result<Vec<Item>> {
        let path = format!(
            "/Users/{}/Items/Resume?Limit={limit}&MediaTypes=Video&Fields={}&EnableImageTypes=Primary,Backdrop,Logo",
            self.user_id,
            Self::FIELDS
        );
        Ok(self.get(&path)?.json::<ItemsResponse>()?.items)
    }

    /// Pick the episode to play for a series: next up, else the first episode.
    /// Next episode to watch. Fresh shows start at the first episode *in watch order*: a special
    /// that Jellyfin / the NFO places before season 1 (displayseason/displayepisode) counts,
    /// unplaced specials (season 0, displayseason -1) do not.
    pub fn episode_for_series(&self, series_id: &str) -> Result<Item> {
        let all = format!(
            "/Shows/{series_id}/Episodes?UserId={}&Fields=SpecialEpisodeNumbers,RunTimeTicks",
            self.user_id
        );
        let items = self.get(&all)?.json::<ItemsResponse>()?.items;
        let started = items.iter().any(|e| e.played() || e.resume_seconds() > 1.0);
        if !started {
            if let Some(first) = items.iter().filter_map(|e| watch_key(e).map(|k| (k, e))).min_by_key(|(k, _)| *k).map(|(_, e)| e) {
                return Ok(first.clone());
            }
        }
        let next = format!(
            "/Shows/NextUp?UserId={}&SeriesId={series_id}&Limit=1&Fields={}",
            self.user_id,
            Self::FIELDS
        );
        if let Some(i) = self.get(&next)?.json::<ItemsResponse>()?.items.into_iter().next() {
            return Ok(i);
        }
        items
            .iter()
            .find(|e| e.parent_index_number != Some(0))
            .or_else(|| items.first())
            .cloned()
            .ok_or_else(|| anyhow!("series has no episodes"))
    }

    pub fn image_url(&self, id: &str, kind: &str, max_height: u32) -> String {
        format!("{}/Items/{id}/Images/{kind}?maxHeight={max_height}&quality=90", self.base)
    }

    /// Decide whether the configured server is on a local/private network
    pub fn is_local(&self) -> bool {
        is_local_url(&self.base)
    }

    /// Ask the server how to play `item`: Direct Play when allowed and local,
    /// bitrate-capped HLS transcode (H.264/HEVC) otherwise
    pub fn play_info(&self, item: &Item, cfg: &Config) -> Result<PlayInfo> {
        let direct = match cfg.stream_mode {
            StreamMode::AlwaysDirect => true,
            StreamMode::AlwaysTranscode => false,
            StreamMode::Auto => self.is_local(),
        };
        let max_bitrate: u64 = if direct { 200_000_000 } else { cfg.remote_bitrate_kbps as u64 * 1000 };

        let body = json!({ "DeviceProfile": device_profile(max_bitrate, direct) });
        let path = format!(
            "/Items/{}/PlaybackInfo?UserId={}&StartTimeTicks={}&MaxStreamingBitrate={max_bitrate}&EnableDirectPlay={direct}&EnableDirectStream={direct}&EnableTranscoding=true&AutoOpenLiveStream=true",
            item.id,
            self.user_id,
            (item.resume_seconds() * 10_000_000.0) as i64
        );
        let v: Value = self.post(&path, &body)?.json()?;
        let session = v["PlaySessionId"].as_str().unwrap_or_default().to_string();
        let src = v["MediaSources"]
            .as_array()
            .and_then(|a| a.first())
            .ok_or_else(|| anyhow!("server returned no media sources"))?;
        let source_id = src["Id"].as_str().unwrap_or(&item.id).to_string();

        let (url, transcoding) = if direct && src["SupportsDirectPlay"].as_bool().unwrap_or(false) {
            (
                format!(
                    "{}/Videos/{}/stream?static=true&MediaSourceId={source_id}&api_key={}&PlaySessionId={session}",
                    self.base, item.id, self.token
                ),
                false,
            )
        } else if let Some(t) = src["TranscodingUrl"].as_str() {
            (format!("{}{}", self.base, t), true)
        } else if src["SupportsDirectStream"].as_bool().unwrap_or(false) {
            (
                format!(
                    "{}/Videos/{}/stream?static=true&MediaSourceId={source_id}&api_key={}",
                    self.base, item.id, self.token
                ),
                false,
            )
        } else {
            return Err(anyhow!("server cannot play or transcode this item"));
        };

        let mut external_subs = Vec::new();
        if let Some(streams) = src["MediaStreams"].as_array() {
            for s in streams {
                if cfg.external_tracks && s["Type"] == "Subtitle" && s["DeliveryMethod"] == "External" {
                    if let Some(u) = s["DeliveryUrl"].as_str() {
                        let full = if u.starts_with("http") { u.to_string() } else { format!("{}{}", self.base, u) };
                        let sep = if full.contains('?') { '&' } else { '?' };
                        external_subs.push((
                            format!("{full}{sep}api_key={}", self.token),
                            s["DisplayTitle"].as_str().unwrap_or("Subtitle").to_string(),
                            s["Language"].as_str().unwrap_or("und").to_string(),
                        ));
                    }
                }
            }
        }

        Ok(PlayInfo {
            item: item.clone(),
            url,
            play_session_id: session,
            media_source_id: source_id,
            transcoding,
            start_seconds: item.resume_seconds(),
            external_subs,
        })
    }

    fn report(&self, endpoint: &str, info: &PlayInfo, position_secs: f64, paused: bool) {
        let body = json!({
            "ItemId": info.item.id,
            "MediaSourceId": info.media_source_id,
            "PlaySessionId": info.play_session_id,
            "PositionTicks": (position_secs * 10_000_000.0) as i64,
            "IsPaused": paused,
            "PlayMethod": if info.transcoding { "Transcode" } else { "DirectPlay" },
        });
        let _ = self.post(endpoint, &body);
    }

    pub fn report_start(&self, info: &PlayInfo) {
        self.report("/Sessions/Playing", info, info.start_seconds, false);
    }
    pub fn report_progress(&self, info: &PlayInfo, pos: f64, paused: bool) {
        self.report("/Sessions/Playing/Progress", info, pos, paused);
    }
    /// Mark an item as watched (removes it from Continue Watching / Next Up progress)
    pub fn mark_played(&self, id: &str) -> Result<()> {
        self.post(&format!("/Users/{}/PlayedItems/{id}", self.user_id), &json!({}))?;
        Ok(())
    }

    pub fn set_favorite(&self, id: &str, fav: bool) -> Result<()> {
        let path = format!("/Users/{}/FavoriteItems/{id}", self.user_id);
        if fav {
            self.post(&path, &json!({}))?;
        } else {
            self.http.delete(format!("{}{}", self.base, path)).header("Authorization", self.auth_header()).send()?.error_for_status()?;
        }
        Ok(())
    }

    /// Random movies and shows from anywhere on the server
    pub fn suggested(&self, limit: u32) -> Vec<Item> {
        let path = format!(
            "/Users/{}/Items?Recursive=true&IncludeItemTypes=Movie,Series&SortBy=Random&Limit={limit}&Fields={}&ImageTypeLimit=1&EnableImageTypes=Primary",
            self.user_id,
            Self::FIELDS
        );
        self.items(&path)
    }

    /// Chapter (name, start seconds) list of an item
    pub fn chapters(&self, item_id: &str) -> Vec<(String, f64)> {
        let path = format!("/Users/{}/Items/{item_id}?Fields=Chapters", self.user_id);
        let Some(v) = self.get(&path).ok().and_then(|r| r.json::<Value>().ok()) else { return vec![] };
        v["Chapters"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|c| (c["Name"].as_str().unwrap_or("").to_string(), c["StartPositionTicks"].as_i64().unwrap_or(0) as f64 / 10_000_000.0))
            .collect()
    }

    pub fn mark_unplayed(&self, id: &str) -> Result<()> {
        self.http
            .delete(format!("{}/Users/{}/PlayedItems/{id}", self.base, self.user_id))
            .header("Authorization", self.auth_header())
            .send()?
            .error_for_status()?;
        Ok(())
    }

    pub fn report_stopped(&self, info: &PlayInfo, pos: f64) {
        self.report("/Sessions/Playing/Stopped", info, pos, false);
    }
}

/// Capability profile: mpv decodes basically everything (H.264, HEVC, AV1, VP9,
/// HDR10/DV base layer, TrueHD/DTS...), so Direct Play accepts any container.
/// When a transcode is needed we ask for HLS H.264/HEVC + AAC/AC3.
fn device_profile(max_bitrate: u64, direct: bool) -> Value {
    let direct_profiles = if direct {
        json!([{ "Type": "Video" }, { "Type": "Audio" }])
    } else {
        // Remote: direct play only when the file already fits under the bitrate cap;
        // the server enforces MaxStreamingBitrate against the source bitrate.
        json!([
            { "Type": "Video", "Container": "mkv,mp4,m4v,webm", "VideoCodec": "h264,hevc,av1", "AudioCodec": "aac,ac3,eac3,opus,flac,mp3" }
        ])
    };
    json!({
        "Name": CLIENT_NAME,
        "MaxStreamingBitrate": max_bitrate,
        "MaxStaticBitrate": max_bitrate,
        "DirectPlayProfiles": direct_profiles,
        "TranscodingProfiles": [{
            "Type": "Video",
            "Container": "ts",
            "Protocol": "hls",
            "Context": "Streaming",
            "VideoCodec": "h264,hevc",
            "AudioCodec": "aac,ac3,eac3",
            "MaxAudioChannels": "6",
            "MinSegments": 1,
            "BreakOnNonKeyFrames": true,
            "EnableSubtitlesInManifest": false
        }],
        // Text subs External (mpv/libass renders them, incl. styled ASS/SSA);
        // bitmap subs Embed so image subs survive direct play and are burned in on transcode.
        "SubtitleProfiles": [
            { "Format": "ass", "Method": "External" },
            { "Format": "ssa", "Method": "External" },
            { "Format": "srt", "Method": "External" },
            { "Format": "vtt", "Method": "External" },
            { "Format": "sub", "Method": "External" },
            { "Format": "pgssub", "Method": "Embed" },
            { "Format": "dvdsub", "Method": "Embed" }
        ]
    })
}

pub fn is_local_url(url: &str) -> bool {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host_port = rest.split('/').next().unwrap_or(rest);
    let host = if let Some(stripped) = host_port.strip_prefix('[') {
        stripped.split(']').next().unwrap_or("")
    } else {
        host_port.split(':').next().unwrap_or("")
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00,
        Err(_) => host.ends_with(".local") || host.ends_with(".lan") || !host.contains('.'),
    }
}

// ------------------------------------------------------------------ segments, admin, next episode

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    Intro,
    Outro,
}

/// An intro/outro range in seconds
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub kind: SegmentKind,
    pub start: f64,
    pub end: f64,
}

impl Client {
    /// Same image endpoint, constrained by width (for 16:9 thumbnails)
    pub fn image_url_w(&self, id: &str, kind: &str, max_width: u32) -> String {
        format!("{}/Items/{id}/Images/{kind}?maxWidth={max_width}&quality=90", self.base)
    }

    /// Horizontal artwork for an item: episode thumbnail, else Thumb/Backdrop
    pub fn thumb_url(&self, item: &Item, max_width: u32) -> String {
        if item.is_episode() {
            if item.image_tags.contains_key("Primary") {
                return self.image_url_w(&item.id, "Primary", max_width);
            }
            return self.image_url_w(item.art_id(), "Backdrop", max_width);
        }
        if item.image_tags.contains_key("Thumb") {
            self.image_url_w(&item.id, "Thumb", max_width)
        } else if !item.backdrop_image_tags.is_empty() {
            self.image_url_w(&item.id, "Backdrop", max_width)
        } else {
            self.image_url_w(&item.id, "Primary", max_width)
        }
    }

    pub fn is_admin(&self) -> bool {
        self.get(&format!("/Users/{}", self.user_id))
            .ok()
            .and_then(|r| r.json::<Value>().ok())
            .and_then(|v| v["Policy"]["IsAdministrator"].as_bool())
            .unwrap_or(false)
    }

    /// Intro/outro ranges. Prefers Jellyfin's native Media Segments API (10.10+, which the
    /// Intro Skipper plugin populates), then falls back to the plugin's own endpoints
    pub fn segments(&self, item: &Item) -> Vec<Segment> {
        let mut out: Vec<Segment> = Vec::new();
        let has = |out: &Vec<Segment>, k: SegmentKind| out.iter().any(|s| s.kind == k);

        let path = format!("/MediaSegments/{}?includeSegmentTypes=Intro&includeSegmentTypes=Outro", item.id);
        if let Some(v) = self.get(&path).ok().and_then(|r| r.json::<Value>().ok()) {
            for s in v["Items"].as_array().cloned().unwrap_or_default() {
                let kind = match s["Type"].as_str() {
                    Some("Intro") => SegmentKind::Intro,
                    Some("Outro") => SegmentKind::Outro,
                    _ => continue,
                };
                let start = s["StartTicks"].as_i64().unwrap_or(0) as f64 / 10_000_000.0;
                let end = s["EndTicks"].as_i64().unwrap_or(0) as f64 / 10_000_000.0;
                if end > start {
                    out.push(Segment { kind, start, end });
                }
            }
        }

        if item.is_episode() && (!has(&out, SegmentKind::Intro) || !has(&out, SegmentKind::Outro)) {
            // Intro Skipper: { "Introduction": {Start,End,Valid}, "Credits": {...} }
            if let Some(v) = self.get(&format!("/Episode/{}/Timestamps", item.id)).ok().and_then(|r| r.json::<Value>().ok()) {
                for (key, kind) in [("Introduction", SegmentKind::Intro), ("Credits", SegmentKind::Outro)] {
                    let o = &v[key];
                    if !o.is_object() || has(&out, kind) || !o["Valid"].as_bool().unwrap_or(true) {
                        continue;
                    }
                    let start = o["Start"].as_f64().or(o["IntroStart"].as_f64()).unwrap_or(0.0);
                    let end = o["End"].as_f64().or(o["IntroEnd"].as_f64()).unwrap_or(0.0);
                    if end > start {
                        out.push(Segment { kind, start, end });
                    }
                }
            }
        }
        if item.is_episode() && !has(&out, SegmentKind::Intro) {
            // Older Intro Skipper: { "IntroStart", "IntroEnd", "Valid" }
            if let Some(v) = self.get(&format!("/Episode/{}/IntroTimestamps/v1", item.id)).ok().and_then(|r| r.json::<Value>().ok()) {
                let start = v["IntroStart"].as_f64().unwrap_or(0.0);
                let end = v["IntroEnd"].as_f64().unwrap_or(0.0);
                if v["Valid"].as_bool().unwrap_or(true) && end > start {
                    out.push(Segment { kind: SegmentKind::Intro, start, end });
                }
            }
        }

        // Chapter names: many releases label their credits/opening as chapters even when no
        // segment provider ran. Used only for kinds the APIs above did not provide.
        if !has(&out, SegmentKind::Intro) || !has(&out, SegmentKind::Outro) {
            let path = format!("/Users/{}/Items/{}?Fields=Chapters", self.user_id, item.id);
            if let Some(v) = self.get(&path).ok().and_then(|r| r.json::<Value>().ok()) {
                let total = v["RunTimeTicks"].as_i64().or(item.run_time_ticks).unwrap_or(0) as f64 / 10_000_000.0;
                let chapters: Vec<(String, f64)> = v["Chapters"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|c| {
                        (
                            c["Name"].as_str().unwrap_or("").to_string(),
                            c["StartPositionTicks"].as_i64().unwrap_or(0) as f64 / 10_000_000.0,
                        )
                    })
                    .collect();
                for (i, (name, start)) in chapters.iter().enumerate() {
                    let Some(kind) = chapter_kind(name) else { continue };
                    if has(&out, kind) {
                        continue;
                    }
                    let end = chapters.get(i + 1).map(|c| c.1).unwrap_or(total);
                    if end > *start + 5.0 {
                        out.push(Segment { kind, start: *start, end });
                    }
                }
            }
        }
        out
    }

    /// Seasons of a series, in order
    pub fn seasons(&self, series_id: &str) -> Vec<Item> {
        let path = format!("/Shows/{series_id}/Seasons?UserId={}&EnableImages=false&EnableUserData=false&Fields=ChildCount", self.user_id);
        self.get(&path)
            .ok()
            .and_then(|r| r.json::<ItemsResponse>().ok())
            .map(|r| r.items)
            .unwrap_or_default()
    }

    /// Episodes of one season, with summaries and watch progress
    pub fn season_episodes(&self, series_id: &str, season_id: &str) -> Vec<Item> {
        let path = format!(
            "/Shows/{series_id}/Episodes?UserId={}&SeasonId={season_id}&Fields={}",
            self.user_id,
            Self::FIELDS
        );
        self.get(&path)
            .ok()
            .and_then(|r| r.json::<ItemsResponse>().ok())
            .map(|r| r.items)
            .unwrap_or_default()
    }

    /// (previous, next) episode around `item` across the whole series
    pub fn adjacent_episodes(&self, item: &Item) -> (Option<Item>, Option<Item>) {
        let Some(series) = item.series_id.as_deref().filter(|_| item.is_episode()) else { return (None, None) };
        let path = format!("/Shows/{series}/Episodes?UserId={}&EnableImages=false", self.user_id);
        let items = match self.get(&path).ok().and_then(|r| r.json::<ItemsResponse>().ok()) {
            Some(r) => r.items,
            None => return (None, None),
        };
        let Some(i) = items.iter().position(|e| e.id == item.id) else { return (None, None) };
        let prev = i.checked_sub(1).and_then(|p| items.get(p)).cloned();
        let next = items.get(i + 1).cloned();
        (prev, next)
    }
}


/// Classify a chapter title as an intro or outro/credits chapter
fn chapter_kind(name: &str) -> Option<SegmentKind> {
    let lower = name.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let has_word = |w: &str| words.iter().any(|x| *x == w);
    let outro = lower.contains("credit")
        || lower.contains("outro")
        || lower.contains("ending")
        || lower.contains("end theme")
        || has_word("ed");
    let intro = lower.contains("intro") || lower.contains("opening") || lower.contains("recap theme") || has_word("op");
    // "Previously on" / "Preview" chapters are neither.
    if outro {
        Some(SegmentKind::Outro)
    } else if intro {
        Some(SegmentKind::Intro)
    } else {
        None
    }
}

// ------------------------------------------------------------------------------ libraries

pub const PAGE_SIZES: [usize; 4] = [25, 50, 75, 100];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Rating,
    Year,
    Length,
}

impl SortKey {
    pub const ALL: [SortKey; 3] = [SortKey::Rating, SortKey::Year, SortKey::Length];

    pub fn name(self) -> &'static str {
        match self {
            SortKey::Rating => "Rating",
            SortKey::Year => "Year",
            SortKey::Length => "Length",
        }
    }
}

impl Client {
    /// The user's libraries, in the order the user set up in Jellyfin (video libraries only)
    pub fn views(&self) -> Vec<Item> {
        self.get(&format!("/Users/{}/Views", self.user_id))
            .ok()
            .and_then(|r| r.json::<ItemsResponse>().ok())
            .map(|r| {
                r.items
                    .into_iter()
                    .filter(|i| {
                        !matches!(
                            i.collection_type.as_deref(),
                            Some("music") | Some("books") | Some("photos") | Some("livetv") | Some("playlists")
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Next episode to watch for shows the user is part-way through (when not in-progress with an episode)
    pub fn next_up_items(&self, limit: u32) -> Vec<Item> {
        let path = format!(
            "/Shows/NextUp?UserId={}&Limit={limit}&Fields={}&EnableResumable=false&EnableImageTypes=Primary,Backdrop,Logo,Thumb",
            self.user_id,
            Self::FIELDS
        );
        self.get(&path)
            .ok()
            .and_then(|r| r.json::<ItemsResponse>().ok())
            .map(|r| r.items)
            .unwrap_or_default()
    }

    /// One page (`page_size` entries) of a library, plus the total number of entries.
    ///
    /// Sorting is done by the server, except "Length" on TV libraries: Jellyfin cannot sort
    /// series by episode count, so those are fetched in full and sorted here.
    pub fn library_page(&self, lib: &Item, key: SortKey, descending: bool, page: usize, page_size: usize) -> Result<(Vec<Item>, usize)> {
        let types = match lib.collection_type.as_deref() {
            Some("movies") => "Movie",
            Some("tvshows") => "Series",
            _ => "Movie,Series",
        };
        let order = if descending { "Descending" } else { "Ascending" };

        if key == SortKey::Length && types == "Series" {
            let path = format!(
                "/Users/{}/Items?ParentId={}&Recursive=true&IncludeItemTypes=Series&Fields=RecursiveItemCount,CommunityRating,ProductionYear,RunTimeTicks&EnableImageTypes=Primary&ImageTypeLimit=1",
                self.user_id, lib.id
            );
            let mut all = self.get(&path)?.json::<ItemsResponse>()?.items;
            all.sort_by(|a, b| {
                a.recursive_item_count
                    .unwrap_or(0)
                    .cmp(&b.recursive_item_count.unwrap_or(0))
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            if descending {
                all.reverse();
            }
            let total = all.len();
            let items = all.into_iter().skip(page * page_size).take(page_size).collect();
            return Ok((items, total));
        }

        let sort_by = match key {
            SortKey::Rating => "CommunityRating,SortName",
            SortKey::Year => "ProductionYear,SortName",
            SortKey::Length => "Runtime,SortName",
        };
        let path = format!(
            "/Users/{}/Items?ParentId={}&Recursive=true&IncludeItemTypes={types}&SortBy={sort_by}&SortOrder={order}&StartIndex={}&Limit={page_size}&Fields={},RecursiveItemCount&EnableTotalRecordCount=true&EnableImageTypes=Primary&ImageTypeLimit=1",
            self.user_id,
            lib.id,
            page * page_size,
            Self::FIELDS
        );
        let r = self.get(&path)?.json::<ItemsResponse>()?;
        let total = r.total_record_count.max(r.items.len());
        Ok((r.items, total))
    }
}

// ------------------------------------------------------------------------------ search / title pages

impl Client {
    fn items(&self, path: &str) -> Vec<Item> {
        self.get(path)
            .ok()
            .and_then(|r| r.json::<ItemsResponse>().ok())
            .map(|r| r.items)
            .unwrap_or_default()
    }

    pub fn search(&self, term: &str) -> SearchResults {
        let q = tokens(term);
        if q.is_empty() {
            return SearchResults::default();
        }
        // server terms: the full text plus the longest words (Jellyfin only does plain substring matching)
        let mut terms = vec![term.trim().to_string()];
        let mut words = q.clone();
        words.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
        for w in words.into_iter().filter(|w| w.chars().count() >= 2).take(2) {
            if !terms.contains(&w) {
                terms.push(w);
            }
        }
        let uid = &self.user_id;
        let f = Self::FIELDS;
        let (mut titles, mut episodes, mut people) = (Vec::<Item>::new(), Vec::<Item>::new(), Vec::<Person>::new());
        std::thread::scope(|sc| {
            let tt = sc.spawn(|| {
                terms.iter().flat_map(|t| self.items(&format!(
                    "/Users/{uid}/Items?SearchTerm={}&IncludeItemTypes=Movie,Series&Recursive=true&Limit=40&Fields={f}&ImageTypeLimit=1&EnableImageTypes=Primary", enc(t)))).collect::<Vec<_>>()
            });
            let et = sc.spawn(|| {
                terms.iter().flat_map(|t| self.items(&format!(
                    "/Users/{uid}/Items?SearchTerm={}&IncludeItemTypes=Episode&Recursive=true&Limit=40&Fields={f}&ImageTypeLimit=1&EnableImageTypes=Primary,Backdrop", enc(t)))).collect::<Vec<_>>()
            });
            let pt = sc.spawn(|| {
                terms.iter().flat_map(|t| self.items(&format!(
                    "/Persons?searchTerm={}&userId={uid}&Limit=40&Fields=PrimaryImageAspectRatio&EnableImages=true", enc(t)))).collect::<Vec<_>>()
            });
            titles = tt.join().unwrap_or_default();
            episodes = et.join().unwrap_or_default();
            people = pt.join().unwrap_or_default().into_iter().map(|i| Person {
                primary_image_tag: i.image_tags.get("Primary").cloned(),
                kind: "Person".into(),
                id: i.id,
                name: i.name,
                role: None,
            }).collect();
        });
        let qs = squash(term);
        let rank = |name: &str| (!squash(name).starts_with(&qs), name.chars().count());
        let mut seen = std::collections::HashSet::new();
        titles.retain(|i| name_matches(&i.name, &q) && seen.insert(i.id.clone()));
        titles.sort_by_key(|i| rank(&i.name));
        titles.truncate(30);
        let mut seen = std::collections::HashSet::new();
        episodes.retain(|i| {
            let hay = format!("{} {}", i.series_name.clone().unwrap_or_default(), i.name);
            name_matches(&hay, &q) && seen.insert(i.id.clone())
        });
        episodes.sort_by_key(|i| rank(&i.name));
        episodes.truncate(30);
        let mut seen = std::collections::HashSet::new();
        people.retain(|p| name_matches(&p.name, &q) && seen.insert(p.id.clone()));
        people.sort_by_key(|p| rank(&p.name));
        people.truncate(30);
        SearchResults { titles, episodes, people }
    }

    /// Movies and shows a person appears in
    pub fn person_titles(&self, person_id: &str) -> Vec<Item> {
        let mut v = self.items(&format!(
            "/Users/{}/Items?PersonIds={person_id}&Recursive=true&IncludeItemTypes=Movie,Series&SortBy=ProductionYear,SortName&SortOrder=Descending&Fields={},People&ImageTypeLimit=1&EnableImageTypes=Primary",
            self.user_id,
            Self::FIELDS
        ));
        for i in v.iter_mut() {
            i.role = i.people.iter().find(|p| p.id == person_id).and_then(|p| p.role.clone()).filter(|r| !r.trim().is_empty());
            i.people.clear();
        }
        v
    }

    /// Extras (behind the scenes, creditless OP/ED...) attached to a movie, show or season
    pub fn extras(&self, id: &str, owner: &str, owner_logo: bool) -> Vec<Item> {
        let mut v = self
            .get(&format!("/Users/{}/Items/{id}/SpecialFeatures", self.user_id))
            .ok()
            .and_then(|r| r.json::<Vec<Item>>().ok())
            .unwrap_or_default();
        for i in v.iter_mut() {
            i.owner = Some((owner.to_string(), owner_logo));
        }
        v
    }

    /// Trickplay (scrub preview) metadata for the item being played, if the server generated it
    pub fn trickplay(&self, item_id: &str, media_source_id: &str) -> Option<Trick> {
        let v: Value = self.get(&format!("/Users/{}/Items/{item_id}", self.user_id)).ok()?.json().ok()?;
        let map = v["Trickplay"].get(media_source_id).or_else(|| v["Trickplay"].as_object()?.values().next())?;
        let (w, info) = map
            .as_object()?
            .iter()
            .filter_map(|(k, v)| Some((k.parse::<u32>().ok()?, v)))
            .min_by_key(|(w, _)| (*w as i64 - 320).abs())?;
        let g = |k: &str| info[k].as_u64().unwrap_or(0);
        let t = Trick {
            item_id: item_id.to_string(),
            media_source_id: media_source_id.to_string(),
            width: w,
            thumb_w: g("Width") as u32,
            thumb_h: g("Height") as u32,
            tile_w: g("TileWidth").max(1) as u32,
            tile_h: g("TileHeight").max(1) as u32,
            count: g("ThumbnailCount") as u32,
            interval_ms: g("Interval").max(1),
        };
        (t.thumb_w > 0 && t.thumb_h > 0 && t.count > 0).then_some(t)
    }

    /// Download and decode one trickplay tile sheet
    pub fn trickplay_tile(&self, t: &Trick, index: u32) -> Option<eframe::egui::ColorImage> {
        let bytes = self
            .get(&format!("/Videos/{}/Trickplay/{}/{index}.jpg?MediaSourceId={}", t.item_id, t.width, t.media_source_id))
            .ok()?
            .bytes()
            .ok()?;
        let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
        let size = [img.width() as usize, img.height() as usize];
        Some(eframe::egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
    }

    pub fn item_detail(&self, id: &str) -> Result<Item> {
        Ok(self.get(&format!("/Users/{}/Items/{id}", self.user_id))?.json::<Item>()?)
    }

    /// Load a movie / show / episode page. Shows start on their first real season.
    pub fn load_title(&self, base: &Item) -> Result<TitleData> {
        let item = self.item_detail(&base.id)?;
        if item.kind == "Episode" {
            let mut item = item;
            let series = item.series_id.clone();
            // no episode-level credits in the metadata: fall back to the show's staff & cast
            if item.people.is_empty() {
                if let Some(s) = &series {
                    if let Ok(show) = self.item_detail(s) {
                        item.people = show.people;
                    }
                }
            }
            let (season_id, episodes) = match (&series, &item.season_id) {
                (Some(sid), Some(season)) => (Some(season.clone()), self.season_episodes(sid, season)),
                _ => (None, vec![]),
            };
            let next_ep = series.as_deref().and_then(|s| self.episode_for_series(s).ok());
            return Ok(TitleData { item, next_ep, similar: vec![], seasons: vec![], season_id, episodes, series_extras: vec![], season_extras: vec![], focus: None });
        }
        let series_extras = self.extras(&item.id, &item.id, item.image_tags.contains_key("Logo"));
        let similar = self.similar(&item.id);
        if item.kind != "Series" {
            return Ok(TitleData { item, next_ep: None, similar, seasons: vec![], season_id: None, episodes: vec![], series_extras, season_extras: vec![], focus: None });
        }
        let seasons = self.seasons(&item.id);
        store_count(&item.id, count_from_seasons(&seasons));
        // open on the season of the last episode watched; fresh shows start on their first real season
        let last = self.last_watched(&item.id);
        let first = last
            .as_ref()
            .and_then(|e| e.season_id.as_ref())
            .and_then(|sid| seasons.iter().find(|s| &s.id == sid))
            .or_else(|| {
                seasons
                    .iter()
                    .filter(|s| s.index_number != Some(0))
                    .min_by_key(|s| s.index_number.unwrap_or(u32::MAX))
            })
            .or_else(|| seasons.first());
        let (season_id, episodes, season_extras) = match first {
            Some(s) => (Some(s.id.clone()), self.season_episodes(&item.id, &s.id), self.extras(&s.id, &item.id, item.image_tags.contains_key("Logo"))),
            None => (None, vec![], vec![]),
        };
        let focus = last.map(|e| e.id);
        let next_ep = self.episode_for_series(&item.id).ok();
        Ok(TitleData { item, next_ep, similar, seasons, season_id, episodes, series_extras, season_extras, focus })
    }

    /// The furthest episode (by season, then number) that has been watched or started
    fn last_watched(&self, series_id: &str) -> Option<Item> {
        let path = format!("/Shows/{series_id}/Episodes?UserId={}&Fields=SpecialEpisodeNumbers,RunTimeTicks", self.user_id);
        let items = self.get(&path).ok()?.json::<ItemsResponse>().ok()?.items;
        items
            .into_iter()
            .filter(|e| e.played() || e.resume_seconds() > 1.0)
            .max_by_key(|e| (e.parent_index_number.unwrap_or(0), e.index_number.unwrap_or(0)))
    }

    /// Jellyfin's "more like this" suggestions
    pub fn similar(&self, id: &str) -> Vec<Item> {
        self.items(&format!("/Items/{id}/Similar?userId={}&limit=14&Fields={}&ImageTypeLimit=1&EnableImageTypes=Primary", self.user_id, Self::FIELDS))
    }

    pub fn server_version(&self) -> String {
        self.http
            .get(format!("{}/System/Info/Public", self.base))
            .send()
            .ok()
            .and_then(|r| r.json::<Value>().ok())
            .and_then(|v| v["Version"].as_str().map(str::to_string))
            .unwrap_or_default()
    }

    /// Episode count of a show, specials excluded
    pub fn fetch_episode_count(&self, series_id: &str) -> Option<u32> {
        count_from_seasons(&self.seasons(series_id))
    }
}

fn count_from_seasons(seasons: &[Item]) -> Option<u32> {
    let real: Vec<_> = seasons.iter().filter(|s| s.index_number != Some(0)).collect();
    if real.is_empty() || real.iter().all(|s| s.child_count.is_none()) {
        return None;
    }
    Some(real.iter().map(|s| s.child_count.unwrap_or(0)).sum())
}

// Episode counts are fetched lazily (one seasons request per show) and cached for the poster cards.
#[derive(Default)]
struct Counts {
    done: HashMap<String, u32>,
    asked: std::collections::HashSet<String>,
    queue: Vec<String>,
}

fn counts() -> &'static std::sync::Mutex<Counts> {
    static C: std::sync::OnceLock<std::sync::Mutex<Counts>> = std::sync::OnceLock::new();
    C.get_or_init(Default::default)
}

/// Cached episode count; queues a background fetch the first time a show is asked about
pub fn ep_count(id: &str) -> Option<u32> {
    let mut c = counts().lock().ok()?;
    if let Some(n) = c.done.get(id) {
        return Some(*n);
    }
    if c.asked.insert(id.to_string()) {
        c.queue.push(id.to_string());
    }
    None
}

pub fn take_count_queue() -> Vec<String> {
    counts().lock().map(|mut c| std::mem::take(&mut c.queue)).unwrap_or_default()
}

pub fn store_count(id: &str, n: Option<u32>) {
    if let (Some(n), Ok(mut c)) = (n, counts().lock()) {
        c.asked.insert(id.to_string());
        c.done.insert(id.to_string(), n);
    }
}

impl Client {
    /// After toggling watched state: fresh detail, next episode and the shown season's episodes
    pub fn refresh_title(&self, id: &str, series_id: Option<&str>, season_id: Option<&str>) -> Result<(Item, Option<Item>, Vec<Item>)> {
        let item = self.item_detail(id)?;
        let series = series_id.unwrap_or(id);
        let is_show_page = item.kind == "Series" || item.kind == "Episode";
        let next = if is_show_page { self.episode_for_series(series).ok() } else { None };
        let eps = match season_id {
            Some(s) if is_show_page => self.season_episodes(series, s),
            _ => vec![],
        };
        Ok((item, next, eps))
    }
}

/// Position of an episode in watch order: (season, episode, tie, special index). `None` for specials
/// that are not placed anywhere.
fn watch_key(e: &Item) -> Option<(i32, i32, i32, i32)> {
    let idx = e.index_number.unwrap_or(0) as i32;
    match e.parent_index_number {
        Some(0) => {
            let season = e.airs_before_season_number.filter(|v| *v >= 0)?;
            let ep = e.airs_before_episode_number.filter(|v| *v >= 0).unwrap_or(0);
            Some((season, ep, -1, idx))
        }
        Some(s) => Some((s as i32, idx, 0, 0)),
        None => None,
    }
}

// ------------------------------------------------------------------------------ forgiving search

fn tokens(s: &str) -> Vec<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).map(str::to_string).collect()
}

fn squash(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Every query word must occur somewhere in the name, ignoring punctuation and spacing
/// ("steins gate 0" finds "Steins;Gate 0").
/// Btw, you should watch that if you see this.
fn name_matches(name: &str, q: &[String]) -> bool {
    let n = squash(name);
    q.iter().all(|t| n.contains(t.as_str()))
}
