use crate::egg_gif;
use eframe::egui::{self, pos2, vec2, Color32, Rect, TextureHandle, TextureOptions};
use std::time::{Duration, Instant};

const ONE_IN: u64 = 10_000;

static GIF: &[u8] = include_bytes!("../assets/egg.gif");
static OGG: &[u8] = include_bytes!("../assets/egg.ogg");

pub fn roll() -> bool {
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new().build_hasher().finish() % ONE_IN == 0
}

pub fn audio_file() -> Option<String> {
    let path = std::env::temp_dir().join("callephiin-egg.ogg");
    std::fs::write(&path, OGG).ok()?;
    Some(path.to_string_lossy().into_owned())
}

pub struct Egg {
    frames: Vec<(TextureHandle, Duration)>,
    total: Duration,
    started: Instant,
}

impl Egg {
    pub fn new(ctx: &egui::Context) -> Option<Self> {
        let decoded = match egg_gif::decode(GIF) {
            Ok(f) if !f.is_empty() => f,
            Ok(_) => return None,
            Err(e) => {
                log::warn!("egg.gif failed to decode: {e}");
                return None;
            }
        };
        let frames: Vec<_> = decoded
            .into_iter()
            .enumerate()
            .map(|(i, f)| {
                let img = egui::ColorImage::from_rgba_unmultiplied([f.width, f.height], &f.rgba);
                (ctx.load_texture(format!("egg_{i}"), img, TextureOptions::NEAREST), f.delay)
            })
            .collect();
        let total = frames.iter().map(|f| f.1).sum();
        Some(Self { frames, total, started: Instant::now() })
    }

    pub fn ui(&self, ctx: &egui::Context) -> bool {
        let mut back = false;
        let mut at = Duration::from_millis((self.started.elapsed().as_millis() % self.total.as_millis().max(1)) as u64);
        let mut shown = 0;
        for (i, (_, d)) in self.frames.iter().enumerate() {
            shown = i;
            if at < *d {
                at = *d - at;
                break;
            }
            at -= *d;
        }
        ctx.request_repaint_after(at);

        egui::CentralPanel::default().frame(egui::Frame::none().fill(Color32::BLACK)).show(ctx, |ui| {
            let tex = &self.frames[shown].0;
            let size = tex.size_vec2() / ctx.pixels_per_point();
            let screen = ui.max_rect();
            let rect = Rect::from_center_size(screen.center(), size);
            ui.painter().image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            let r = Rect::from_min_size(pos2(24.0, 18.0), vec2(48.0, 48.0));
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(r), |ui| {
                if crate::ui::icon_button(ui, crate::ui::Icon::Back, 40.0, true).clicked() {
                    back = true;
                }
            });
        });
        back
    }
}
