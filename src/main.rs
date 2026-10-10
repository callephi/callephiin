#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod app;
mod config;
mod debuglog;
mod discord;
mod imgloader;
mod sonarr;
use debuglog::LogOk;
mod jellyfin;
mod player;
mod ui;



fn load_icon() -> Option<eframe::egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../assets/icon.ico")).log_ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    Some(eframe::egui::IconData { rgba: img.into_raw(), width, height })
}

fn main() -> eframe::Result<()> {
    debuglog::init(config::Config::load().debug_log);
    // write panics to a file next to the exe (the release build has no console window)
    std::panic::set_hook(Box::new(|info| {
        let path = std::env::current_exe().log_ok().and_then(|p| p.parent().map(|d| d.join("callephiin-crash.log")));
        if let Some(p) = path {
            crate::logged!(std::fs::write(p, format!("{info}\n{}", std::backtrace::Backtrace::force_capture())));
            log::error!("panic: {info}");
        }
    }));
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("callephiin")
        .with_app_id("callephiin")
        .with_inner_size([1280.0, 720.0])
        .with_min_inner_size([900.0, 560.0]);
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        // always open at the default 720p-sized window, centred (not at whatever size it was closed at)
        centered: true,
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native("callephiin", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
