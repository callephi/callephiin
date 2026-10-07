use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// callephiin's Discord application ID.
const APP_ID: &str = "1556464248538800128";

/// Snapshot of what is playing; the UI thread keeps this up to date.
#[derive(Clone, Debug, PartialEq)]
pub struct NowPlaying {
    /// Show or movie title.
    pub title: String,
    /// "S1:E3 · Episode Title" for episodes, empty for movies.
    pub detail: String,
    pub position: f64,
    pub paused: bool,
}

pub type Shared = Arc<Mutex<Option<NowPlaying>>>;

const REFRESH: Duration = Duration::from_secs(30);

fn fmt_time(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Background thread that pushes Rich Presence: immediately when the title or
/// pause state changes, and otherwise every 30 seconds.
pub fn spawn(enabled: Arc<AtomicBool>, shared: Shared) {
    thread::spawn(move || {
        let mut client: Option<DiscordIpcClient> = None;
        let mut last_sent: Option<(Instant, String, String, bool)> = None;

        loop {
            thread::sleep(Duration::from_secs(1));

            let now_playing = shared.lock().ok().and_then(|g| g.clone());
            let active = enabled.load(Ordering::Relaxed);

            let Some(np) = now_playing.filter(|_| active) else {
                if let Some(mut c) = client.take() {
                    let _ = c.clear_activity();
                    let _ = c.close();
                }
                last_sent = None;
                continue;
            };

            let changed = match &last_sent {
                None => true,
                Some((t, title, detail, paused)) => {
                    *title != np.title || *detail != np.detail || *paused != np.paused || t.elapsed() >= REFRESH
                }
            };
            if !changed {
                continue;
            }

            if client.is_none() {
                if let Ok(mut c) = DiscordIpcClient::new(APP_ID) {
                    if c.connect().is_ok() {
                        client = Some(c);
                    }
                }
            }
            let Some(c) = client.as_mut() else {
                // Discord not running; retry on the next refresh interval.
                last_sent = Some((Instant::now(), np.title.clone(), np.detail.clone(), np.paused));
                continue;
            };

            let state = if np.paused {
                if np.detail.is_empty() {
                    format!("Paused at {}", fmt_time(np.position))
                } else {
                    format!("{} · Paused at {}", np.detail, fmt_time(np.position))
                }
            } else if np.detail.is_empty() {
                "Watching a movie".to_string()
            } else {
                np.detail.clone()
            };

            let mut act = activity::Activity::new().details(&np.title).state(&state);
            if !np.paused {
                // Discord counts up from this start time => shows current playback time.
                let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
                act = act.timestamps(activity::Timestamps::new().start(now - np.position as i64));
            }

            if c.set_activity(act).is_err() {
                // Connection dropped; reconnect on next tick.
                if let Some(mut dead) = client.take() {
                    let _ = dead.close();
                }
            } else {
                last_sent = Some((Instant::now(), np.title.clone(), np.detail.clone(), np.paused));
            }
        }
    });
}
