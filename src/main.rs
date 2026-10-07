#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod app;
mod config;
mod discord;
mod jellyfin;
mod player;
mod ui;

fn load_icon() -> Option<eframe::egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../assets/icon.ico")).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    Some(eframe::egui::IconData { rgba: img.into_raw(), width, height })
}

fn main() -> eframe::Result<()> {
    env_logger::init();
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("callephiin")
        .with_app_id("callephiin")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([900.0, 560.0]);
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native("callephiin", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
