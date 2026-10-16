//! Minimal Sonarr client: connection test, next air date and alternate titles for one show.
use chrono::{DateTime, Local, Utc};
use serde_json::Value;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Next {
    pub season: u32,
    pub episode: u32,
    pub at: DateTime<Utc>,
    /// "Series finale", "Season 2 finale", "Midseason finale" when Sonarr marks it so
    pub finale: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Info {
    pub next: Option<Next>,
    /// Alternate titles known to Sonarr (no language labels in its API)
    pub aka: Vec<String>,
}

impl Next {
    /// "Episode 2 airs in 10 hours" / "S2:E1 airs on Oct 10, 2026"
    pub fn text(&self) -> Option<String> {
        let mins = (self.at - Utc::now()).num_minutes();
        if mins < 0 {
            return None;
        }
        let what = if let Some(f) = &self.finale {
            f.clone()
        } else if self.season == 1 { format!("Episode {}", self.episode) } else { format!("S{}:E{}", self.season, self.episode) };
        let plural = |n: i64, unit: &str| if n == 1 { format!("1 {unit}") } else { format!("{n} {unit}s") };
        let when = if mins < 60 {
            format!("in {}", plural(mins.max(1), "minute"))
        } else if mins < 24 * 60 {
            format!("in {}", plural((mins + 30) / 60, "hour"))
        } else {
            format!("on {}", self.at.with_timezone(&Local).format("%b %-d, %Y"))
        };
        Some(format!("{what} airs {when}"))
    }
}

fn base(url: &str) -> String {
    let u = url.trim().trim_end_matches('/');
    if u.starts_with("http://") || u.starts_with("https://") { u.to_string() } else { format!("http://{u}") }
}

fn get(url: &str, key: &str, path: &str) -> Result<Value, String> {
    let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(8)).build().map_err(|e| e.to_string())?;
    let resp = client.get(format!("{}{path}", base(url))).header("X-Api-Key", key.trim()).send().map_err(|e| format!("could not reach Sonarr: {e}"))?;
    match resp.status().as_u16() {
        200..=299 => resp.json::<Value>().map_err(|e| format!("unexpected reply: {e}")),
        401 | 403 => Err("API key rejected".into()),
        c => Err(format!("HTTP {c}")),
    }
}

/// Checks address and key; returns a short success message.
pub fn test(url: &str, key: &str) -> Result<String, String> {
    if url.trim().is_empty() || key.trim().is_empty() {
        return Err("enter an address and an API key".into());
    }
    let v = get(url, key, "/api/v3/system/status")?;
    Ok(format!("Connected to Sonarr {}", v["version"].as_str().unwrap_or("")).trim().to_string())
}

/// Finds the show (by TVDB id, else by title) and reads its next unaired episode.
pub fn lookup(url: &str, key: &str, tvdb: Option<&str>, title: &str) -> Result<Option<Info>, String> {
    let mut found: Option<Value> = None;
    if let Some(id) = tvdb {
        found = get(url, key, &format!("/api/v3/series?tvdbId={id}"))?.as_array().and_then(|a| a.first().cloned());
    }
    if found.is_none() {
        let all = get(url, key, "/api/v3/series")?;
        found = all.as_array().and_then(|a| a.iter().find(|s| s["title"].as_str().map_or(false, |t| t.eq_ignore_ascii_case(title))).cloned());
    }
    log::debug!("sonarr: looking up '{title}' (tvdb {tvdb:?}): {}", if found.is_some() { "found" } else { "not found" });
    let Some(series) = found else { return Ok(None) };
    let Some(sid) = series["id"].as_i64() else { return Ok(None) };

    let now = Utc::now();
    let mut next: Option<Next> = None;
    if let Some(eps) = get(url, key, &format!("/api/v3/episode?seriesId={sid}"))?.as_array() {
        for e in eps {
            let (Some(s), Some(n), Some(at)) = (e["seasonNumber"].as_u64(), e["episodeNumber"].as_u64(), e["airDateUtc"].as_str().and_then(|d| DateTime::parse_from_rfc3339(d).ok())) else { continue };
            let at = at.with_timezone(&Utc);
            if s == 0 || at < now || next.as_ref().map_or(false, |x| x.at <= at) {
                continue;
            }
            let finale = match e["finaleType"].as_str() {
                Some("series") => Some("Series finale".to_string()),
                Some("season") => Some(format!("Season {s} finale")),
                Some("midseason") => Some(format!("Season {s} midseason finale")),
                _ => None,
            };
            next = Some(Next { season: s as u32, episode: n as u32, at, finale });
        }
    }
    log::debug!("sonarr: next episode for '{title}': {next:?}");
    let mut aka: Vec<String> = Vec::new();
    for t in series["alternateTitles"].as_array().into_iter().flatten().filter_map(|t| t["title"].as_str()) {
        let same = |x: &str| x.eq_ignore_ascii_case(t);
        if !same(title) && !aka.iter().any(|x| same(x)) {
            aka.push(t.to_string());
        }
    }
    aka.truncate(6);
    Ok(Some(Info { next, aka }))
}

/// One upcoming episode on the calendar.
#[derive(Clone, Debug)]
pub struct CalEntry {
    pub show: String,
    pub season: u32,
    pub episode: u32,
    /// Day it airs (local day when the time is known, Sonarr's air date otherwise)
    pub date: chrono::NaiveDate,
    /// Local air time; `None` when Sonarr doesn't know it
    pub time: Option<chrono::NaiveTime>,
}

/// Episodes airing from `start` to `end` (inclusive) for the shows Sonarr monitors.
pub fn calendar(url: &str, key: &str, start: chrono::NaiveDate, end: chrono::NaiveDate) -> Result<Vec<CalEntry>, String> {
    let end = end + chrono::Duration::days(1);
    let v = get(url, key, &format!("/api/v3/calendar?start={start}&end={end}&includeSeries=true"))?;
    let mut out = Vec::new();
    for e in v.as_array().into_iter().flatten() {
        let (Some(season), Some(episode)) = (e["seasonNumber"].as_u64(), e["episodeNumber"].as_u64()) else { continue };
        let show = e["series"]["title"].as_str().unwrap_or("").to_string();
        if show.is_empty() {
            continue;
        }
        // a time only counts when the series has an air time; otherwise Sonarr's UTC value is a guess
        let has_time = e["series"]["airTime"].as_str().map_or(false, |t| !t.trim().is_empty());
        let utc = e["airDateUtc"].as_str().and_then(|d| DateTime::parse_from_rfc3339(d).ok());
        let (date, time) = match (utc, has_time) {
            (Some(u), true) => {
                let l = u.with_timezone(&Local);
                (l.date_naive(), Some(l.time()))
            }
            _ => match e["airDate"].as_str().and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) {
                Some(d) => (d, None),
                None => continue,
            },
        };
        out.push(CalEntry { show, season: season as u32, episode: episode as u32, date, time });
    }
    out.sort_by(|a, b| (a.date, a.time, &a.show).cmp(&(b.date, b.time, &b.show)));
    log::debug!("sonarr calendar {start}..{end}: {} episode(s)", out.len());
    Ok(out)
}
