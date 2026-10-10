//! Optional file logging, switched on from Settings ("Enable debug log output"), so debugging
//! never needs a console window. Off by default.
use directories::ProjectDirs;
use log::{LevelFilter, Log, Metadata, Record};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

struct FileLogger {
    file: Mutex<Option<std::fs::File>>,
}

static LOGGER: FileLogger = FileLogger { file: Mutex::new(None) };

fn dir() -> Option<PathBuf> {
    // <app folder>/logs, next to the config folder
    ProjectDirs::from("", "", "callephiin").and_then(|d| d.config_dir().parent().map(|p| p.join("logs")))
}

/// callephiin's own log.
pub fn path() -> Option<PathBuf> {
    dir().map(|d| d.join("debug.log"))
}

/// libmpv's log (picked up when the player is created, i.e. after a restart).
pub fn mpv_path() -> Option<PathBuf> {
    dir().map(|d| d.join("mpv.log"))
}

pub fn init(enabled: bool) {
    // each launch starts a clean log; the previous one is kept as debug-old.log
    if let Some(p) = path() {
        if p.exists() {
            let old = p.with_file_name("debug-old.log");
            let _ = std::fs::remove_file(&old);
            let _ = std::fs::rename(&p, &old);
        }
    }
    let _ = log::set_logger(&LOGGER);
    set_enabled(enabled);
}

pub fn set_enabled(on: bool) {
    let Ok(mut f) = LOGGER.file.lock() else { return };
    if on {
        if let Some(p) = path() {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            *f = std::fs::OpenOptions::new().create(true).append(true).open(p).ok();
        }
        log::set_max_level(LevelFilter::Debug);
        if let Some(file) = f.as_mut() {
            let _ = writeln!(file, "---- callephiin v{} debug log started ----", env!("CARGO_PKG_VERSION"));
        }
    } else {
        log::set_max_level(LevelFilter::Off);
        *f = None;
    }
}

impl Log for FileLogger {
    fn enabled(&self, m: &Metadata) -> bool {
        // our own messages at any level; dependencies only when they warn
        m.target().starts_with("callephiin") || m.level() <= log::Level::Warn
    }

    fn log(&self, r: &Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        if let Ok(mut f) = self.file.lock() {
            if let Some(file) = f.as_mut() {
                let _ = writeln!(file, "{} {:5} {}: {}", chrono::Local::now().format("%H:%M:%S%.3f"), r.level(), r.target(), r.args());
            }
        }
    }

    fn flush(&self) {}
}

/// Log a failed result (and carry on). Use through [`logged!`].
pub fn check<T, E: std::fmt::Debug>(r: Result<T, E>, what: &str) {
    if let Err(e) = r {
        log::warn!("{what} failed: {e:?}");
    }
}

/// `let _ = expr;` replacement that records the error instead of dropping it.
#[macro_export]
macro_rules! logged {
    ($e:expr) => {
        $crate::debuglog::check($e, stringify!($e))
    };
}

/// `.ok()` that writes the error (with the calling file and line) to the debug log.
pub trait LogOk<T> {
    fn log_ok(self) -> Option<T>;
}

impl<T, E: std::fmt::Debug> LogOk<T> for Result<T, E> {
    #[track_caller]
    fn log_ok(self) -> Option<T> {
        match self {
            Ok(v) => Some(v),
            Err(e) => {
                let at = std::panic::Location::caller();
                log::warn!("{}:{}: {e:?}", at.file().rsplit(['/', '\\']).next().unwrap_or(""), at.line());
                None
            }
        }
    }
}
