//! Reusable widgets and painting helpers for the callephiin UI.
//! Everything that takes `s` scales with the content scale factor.

use crate::jellyfin::{Client, Item};
use crate::player::Track;
use std::collections::HashMap;
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, FontId, Layout, Pos2, Rect, RichText, Rounding, Sense, Shape, Stroke, UiBuilder};

pub const BG: Color32 = Color32::from_rgb(0x0e, 0x10, 0x16);
pub const PANEL: Color32 = Color32::from_rgb(0x15, 0x18, 0x20);
pub const ACCENT: Color32 = Color32::from_rgb(0x8a, 0x6b, 0xff);
pub const MUTED: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xb0);

/// Scale fonts and spacing of this `Ui` (and children) by `s`.
pub fn scale_style(ui: &mut egui::Ui, s: f32) {
    let st = ui.style_mut();
    for (_, f) in st.text_styles.iter_mut() {
        f.size *= s;
    }
    let sp = &mut st.spacing;
    sp.item_spacing *= s;
    sp.button_padding *= s;
    sp.interact_size *= s;
    sp.slider_width *= s;
    sp.combo_width *= s;
    sp.text_edit_width *= s;
    sp.icon_width *= s;
    sp.icon_width_inner *= s;
    sp.indent *= s;
}

/// The callephiin logo (SVG), scaled to fit a `size` x `size` box regardless of the SVG's own
/// dimensions or aspect ratio. It is rasterised here at twice the physical size (so the GPU's
/// 2:1 bilinear downscale is smooth) instead of letting the image loader pick a size.
pub fn logo(ui: &mut egui::Ui, size: f32) {
    thread_local! {
        static LOGOS: std::cell::RefCell<HashMap<u32, egui::TextureHandle>> = Default::default();
    }
    let px = ((size * ui.ctx().pixels_per_point() * 2.0).round() as u32).clamp(32, 2048);
    let tex = LOGOS.with(|m| {
        let mut m = m.borrow_mut();
        if !m.contains_key(&px) {
            if let Some(img) = render_logo(px) {
                m.insert(px, ui.ctx().load_texture(format!("logo{px}"), img, egui::TextureOptions::LINEAR));
            }
        }
        m.get(&px).map(|t| t.id())
    });
    match tex {
        Some(id) => {
            ui.add(egui::Image::new(egui::load::SizedTexture::new(id, vec2(size, size))));
        }
        None => {
            ui.add(
                egui::Image::new(egui::include_image!("../assets/logo.svg"))
                    .fit_to_exact_size(vec2(size, size))
                    .maintain_aspect_ratio(true),
            );
        }
    }
}

fn render_logo(px: u32) -> Option<egui::ColorImage> {
    let bytes = include_bytes!("../assets/logo.svg");
    use resvg::usvg::TreeParsing;
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok()?;
    let sz = tree.size;
    let scale = (px as f32 / sz.width()).min(px as f32 / sz.height());
    let mut pm = resvg::tiny_skia::Pixmap::new(px, px)?;
    let tx = (px as f32 - sz.width() * scale) / 2.0;
    let ty = (px as f32 - sz.height() * scale) / 2.0;
    resvg::Tree::from_usvg(&tree).render(resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(tx, ty), &mut pm.as_mut());
    Some(egui::ColorImage::from_rgba_premultiplied([px as usize, px as usize], pm.data()))
}

pub fn nav_button(ui: &mut egui::Ui, icon: &str, label: &str, selected: bool, height: f32, compact: bool) -> bool {
    let height = if compact { 46.0 } else { height };
    let (rect, resp) = ui.allocate_exact_size(vec2(if compact { 38.0 } else { 68.0 }, height), Sense::click());
    if selected || resp.hovered() {
        let fill = if selected { ACCENT.gamma_multiply(0.28) } else { Color32::from_white_alpha(10) };
        ui.painter().rect_filled(rect, 14.0, fill);
    }
    if selected {
        ui.painter()
            .rect_filled(Rect::from_min_size(rect.left_top() + vec2(-10.0, 14.0), vec2(4.0, height - 28.0)), 2.0, ACCENT);
    }
    let color = if selected { Color32::WHITE } else { MUTED };
    if icon == "search" {
        let c = pos2(rect.center().x - 1.5, if compact { rect.center().y - 1.5 } else { rect.top() + 20.5 });
        let st = Stroke::new(2.2_f32, color);
        ui.painter().circle_stroke(c, 7.0, st);
        ui.painter().line_segment([c + vec2(5.0, 5.0), c + vec2(11.0, 11.0)], st);
    } else if icon == "home" {
        let c = pos2(rect.center().x, if compact { rect.center().y } else { rect.top() + 22.0 });
        let st = Stroke::new(2.0_f32, color);
        let (l, r, top, bot) = (c.x - 9.0, c.x + 9.0, c.y - 10.0, c.y + 9.0);
        let wall = c.y - 1.0;
        ui.painter().add(Shape::line(vec![pos2(l - 2.0, wall), pos2(c.x, top), pos2(r + 2.0, wall)], st));
        ui.painter().add(Shape::line(
            vec![pos2(l + 1.5, wall - 1.0), pos2(l + 1.5, bot), pos2(r - 1.5, bot), pos2(r - 1.5, wall - 1.0)],
            st,
        ));
        ui.painter().rect_filled(Rect::from_center_size(pos2(c.x, bot - 3.5), vec2(4.0, 7.0)), 1.0, color);
    } else {
        ui.painter().text(
            pos2(rect.center().x, if compact { rect.center().y } else { rect.top() + 22.0 }),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(22.0),
            color,
        );
    }
    // Each line of the label is painted on its own so multi-line labels stay centred.
    for (i, line) in label.lines().enumerate().filter(|_| !compact) {
        ui.painter().text(
            pos2(rect.center().x, rect.top() + 44.0 + i as f32 * 13.0),
            Align2::CENTER_CENTER,
            line,
            FontId::proportional(11.5),
            color,
        );
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

pub fn section(ui: &mut egui::Ui, title: &str, s: f32, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none().fill(PANEL).rounding(14.0 * s).inner_margin(16.0 * s).show(ui, |ui| {
        ui.set_width(ui.available_width().min(760.0 * s));
        ui.label(RichText::new(title).size(17.0 * s).strong());
        ui.add_space(6.0 * s);
        body(ui);
    });
    ui.add_space(12.0 * s);
}

// ------------------------------------------------------------------------------ buttons

fn brighten(c: Color32) -> Color32 {
    if c.a() < 255 {
        return c;
    }
    Color32::from_rgb(c.r().saturating_add(22), c.g().saturating_add(22), c.b().saturating_add(22))
}

/// A rounded button whose label is always centred and whose width follows the label
/// (never narrower than `min_w`).
#[allow(clippy::too_many_arguments)]
pub fn pill_button(
    ui: &mut egui::Ui,
    label: &str,
    text_size: f32,
    height: f32,
    pad_x: f32,
    min_w: f32,
    rounding: f32,
    fill: Color32,
    text: Color32,
) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), FontId::proportional(text_size), text);
    let w = (galley.size().x + 2.0 * pad_x).max(min_w);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, height), Sense::click());
    let fill = if resp.hovered() { brighten(fill) } else { fill };
    ui.painter().rect_filled(rect, rounding, fill);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, text);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    SkipBack,
    SkipFwd,
    Play,
    Pause,
    Subtitles,
    Audio,
    Volume,
    Back,
    Prev,
    Next,
    Fullscreen,
    ExitFullscreen,
    Episodes,
}

/// Square icon button drawn with the painter (no font glyphs needed).
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, size: f32, enabled: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), if enabled { Sense::click() } else { Sense::hover() });
    let hot = resp.hovered() && enabled;
    if hot {
        ui.painter().rect_filled(rect, 8.0, Color32::from_white_alpha(28));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let col = if !enabled {
        Color32::from_white_alpha(70)
    } else if hot {
        Color32::WHITE
    } else {
        Color32::from_gray(215)
    };
    let c = rect.center();
    let p = ui.painter();
    let stroke = Stroke::new(2.0_f32, col);
    match icon {
        Icon::SkipBack | Icon::SkipFwd => {
            let d = if icon == Icon::SkipBack { -1.0_f32 } else { 1.0 };
            for k in [-0.5_f32, 0.5] {
                let base = c.x + k * 0.26 * size;
                let tip = pos2(base + d * 0.14 * size, c.y);
                let back = base - d * 0.14 * size;
                p.add(Shape::convex_polygon(vec![tip, pos2(back, c.y - 0.2 * size), pos2(back, c.y + 0.2 * size)], col, Stroke::NONE));
            }
        }
        Icon::Play => {
            let (w, h) = (0.17 * size, 0.22 * size);
            p.add(Shape::convex_polygon(
                vec![pos2(c.x - w * 0.8, c.y - h), pos2(c.x + w * 1.4, c.y), pos2(c.x - w * 0.8, c.y + h)],
                col,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            for dx in [-0.11_f32, 0.11] {
                p.rect_filled(Rect::from_center_size(pos2(c.x + dx * size, c.y), vec2(0.1 * size, 0.38 * size)), 1.5, col);
            }
        }
        Icon::Subtitles => {
            let body = Rect::from_center_size(pos2(c.x, c.y - 0.03 * size), vec2(0.5 * size, 0.34 * size));
            p.rect_stroke(body, 0.08 * size, stroke);
            // tail
            p.add(Shape::line(
                vec![pos2(c.x - 0.12 * size, body.bottom()), pos2(c.x - 0.14 * size, c.y + 0.28 * size), pos2(c.x + 0.02 * size, body.bottom())],
                stroke,
            ));
            for dx in [-0.1_f32, 0.0, 0.1] {
                p.circle_filled(pos2(c.x + dx * size, body.center().y), 1.3, col);
            }
        }
        Icon::Audio => {
            for (i, hgt) in [0.14_f32, 0.3, 0.46, 0.24, 0.36].iter().enumerate() {
                let x = c.x + (i as f32 - 2.0) * 0.11 * size;
                p.line_segment([pos2(x, c.y - hgt * size / 2.0), pos2(x, c.y + hgt * size / 2.0)], Stroke::new(2.4_f32, col));
            }
        }
        Icon::Volume => {
            let x = c.x - 0.2 * size;
            p.add(Shape::convex_polygon(
                vec![
                    pos2(x, c.y - 0.08 * size),
                    pos2(x + 0.1 * size, c.y - 0.08 * size),
                    pos2(x + 0.24 * size, c.y - 0.2 * size),
                    pos2(x + 0.24 * size, c.y + 0.2 * size),
                    pos2(x + 0.1 * size, c.y + 0.08 * size),
                    pos2(x, c.y + 0.08 * size),
                ],
                col,
                Stroke::NONE,
            ));
            for (k, r) in [0.13_f32, 0.24].iter().enumerate() {
                let pts: Vec<_> = (-4..=4)
                    .map(|i| {
                        let a = i as f32 * 0.14;
                        pos2(x + 0.26 * size + r * size * a.cos() * 0.8 - k as f32 * 0.0, c.y + r * size * a.sin())
                    })
                    .collect();
                p.add(Shape::line(pts, Stroke::new(1.8_f32, col)));
            }
        }
        Icon::Back => {
            let w = 0.14 * size;
            let h = 0.24 * size;
            let x = c.x + w * 0.5;
            let st = Stroke::new(2.8_f32, col);
            p.line_segment([pos2(x, c.y - h), pos2(x - 2.0 * w, c.y)], st);
            p.line_segment([pos2(x - 2.0 * w, c.y), pos2(x, c.y + h)], st);
        }
        Icon::Prev | Icon::Next => {
            let d = if icon == Icon::Prev { -1.0 } else { 1.0 };
            // bar on the outer side, triangle pointing outwards
            let bar_x = c.x + d * 0.22 * size;
            p.line_segment([pos2(bar_x, c.y - 0.2 * size), pos2(bar_x, c.y + 0.2 * size)], Stroke::new(2.5_f32, col));
            let tip = pos2(c.x + d * 0.14 * size, c.y);
            let back_x = c.x - d * 0.2 * size;
            p.add(Shape::convex_polygon(
                vec![tip, pos2(back_x, c.y - 0.2 * size), pos2(back_x, c.y + 0.2 * size)],
                col,
                Stroke::NONE,
            ));
        }
        Icon::Fullscreen | Icon::ExitFullscreen => {
            let arm = 0.13 * size;
            for (sx, sy) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                if icon == Icon::Fullscreen {
                    // corner brackets at the outer corners
                    let q = pos2(c.x + sx * 0.28 * size, c.y + sy * 0.28 * size);
                    p.line_segment([q, pos2(q.x - sx * arm, q.y)], stroke);
                    p.line_segment([q, pos2(q.x, q.y - sy * arm)], stroke);
                } else {
                    // brackets pulled towards the centre, arms pointing outwards
                    let q = pos2(c.x + sx * 0.13 * size, c.y + sy * 0.13 * size);
                    p.line_segment([q, pos2(q.x + sx * arm, q.y)], stroke);
                    p.line_segment([q, pos2(q.x, q.y + sy * arm)], stroke);
                }
            }
        }
        Icon::Episodes => {
            for dy in [-0.2_f32, 0.0, 0.2] {
                let y = c.y + dy * size;
                p.circle_filled(pos2(c.x - 0.26 * size, y), 0.035 * size + 0.8, col);
                p.line_segment([pos2(c.x - 0.14 * size, y), pos2(c.x + 0.28 * size, y)], stroke);
            }
        }
    }
    resp
}

// ------------------------------------------------------------------------------ seek bar

/// A highlighted range on the seek bar (intro, credits...).
pub struct SeekMark {
    pub start: f64,
    pub end: f64,
    pub label: &'static str,
}

pub enum SeekEvent {
    None,
    /// Pointer is dragging; preview position (seconds).
    Drag(f64),
    /// Pointer released / clicked; seek here (seconds).
    Commit(f64),
}

/// Time bar with intro/credits ranges drawn on the track.
pub fn seek_bar(ui: &mut egui::Ui, width: f32, value: f64, total: f64, marks: &[SeekMark]) -> SeekEvent {
    let total = total.max(1.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click_and_drag());
    let active = resp.hovered() || resp.dragged();
    let th = if active { 8.0 } else { 5.0 };
    let tr = Rect::from_center_size(rect.center(), vec2(rect.width(), th));
    let x_of = |t: f64| tr.left() + ((t / total).clamp(0.0, 1.0) as f32) * tr.width();

    let p = ui.painter();
    p.rect_filled(tr, th / 2.0, Color32::from_white_alpha(60));
    let px = x_of(value);
    p.rect_filled(Rect::from_min_max(tr.min, pos2(px, tr.max.y)), th / 2.0, ACCENT);
    for m in marks {
        let (a, b) = (x_of(m.start), x_of(m.end));
        let band = Rect::from_min_max(pos2(a, tr.min.y), pos2(b.max(a + 2.0), tr.max.y));
        p.rect_filled(band, 1.5, Color32::from_rgba_unmultiplied(255, 200, 70, 190));
        for x in [a, b] {
            p.line_segment(
                [pos2(x, tr.min.y - 3.0), pos2(x, tr.max.y + 3.0)],
                Stroke::new(1.5_f32, Color32::from_rgba_unmultiplied(255, 225, 140, 235)),
            );
        }
    }
    p.circle_filled(pos2(px, rect.center().y), if active { 9.0 } else { 6.5 }, Color32::WHITE);

    let pointer_t = resp
        .interact_pointer_pos()
        .or_else(|| resp.hover_pos())
        .map(|pos| (((pos.x - tr.left()) / tr.width()).clamp(0.0, 1.0) as f64) * total);

    if resp.hovered() && !resp.dragged() {
        if let Some(t) = pointer_t {
            let text = match marks.iter().find(|m| t >= m.start && t <= m.end) {
                Some(m) => format!("{}  ·  {}", fmt_time(t), m.label),
                None => fmt_time(t),
            };
            resp.clone().on_hover_text_at_pointer(text);
        }
    }

    if resp.drag_stopped() || resp.clicked() {
        return SeekEvent::Commit(pointer_t.unwrap_or(value));
    }
    if resp.dragged() {
        return SeekEvent::Drag(pointer_t.unwrap_or(value));
    }
    SeekEvent::None
}

// ------------------------------------------------------------------------------ cards

/// Draw `text` at `pos`, wrapped to `max_w`, at most `max_rows` rows, with an ellipsis if cut.
/// Painting is clipped to `clip`, so nothing can spill into neighbouring cards.
pub fn paint_wrapped(ui: &egui::Ui, clip: Rect, pos: Pos2, text: &str, font: FontId, color: Color32, max_w: f32, max_rows: usize) {
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, color, max_w);
    job.wrap.max_rows = max_rows;
    job.wrap.overflow_character = Some('…');
    let galley = ui.painter().layout_job(job);
    ui.painter().with_clip_rect(clip.intersect(ui.clip_rect())).galley(pos, galley, color);
}

pub fn row_height(ui: &egui::Ui, font: &FontId) -> f32 {
    ui.fonts(|f| f.row_height(font))
}

/// Sizes of one poster card (poster, two title lines, year/score line).
struct PosterMetrics {
    pw: f32,
    ph: f32,
    total_h: f32,
    title_h: f32,
    title_font: FontId,
    sub_font: FontId,
}

fn poster_metrics(ui: &egui::Ui, s: f32) -> PosterMetrics {
    let title_font = FontId::proportional(14.0 * s);
    let sub_font = FontId::proportional(12.0 * s);
    let (pw, ph) = (150.0 * s, 225.0 * s);
    let title_h = row_height(ui, &title_font) * 2.0;
    let sub_h = row_height(ui, &sub_font);
    PosterMetrics { pw, ph, total_h: ph + 8.0 * s + title_h + 3.0 * s + sub_h + 4.0 * s, title_h, title_font, sub_font }
}

fn poster_card(ui: &mut egui::Ui, client: &Client, item: &Item, s: f32, m: &PosterMetrics, play: &mut Option<Item>, hover: &mut Option<Item>) {
    let (rect, resp) = ui.allocate_exact_size(vec2(m.pw, m.total_h), Sense::click());
    let poster = Rect::from_min_size(rect.min, vec2(m.pw, m.ph));
    ui.painter().rect_filled(poster, 12.0 * s, PANEL);
    egui::Image::new(client.image_url(item.art_id(), "Primary", (450.0 * s) as u32)).show_loading_spinner(false)
        .rounding(12.0 * s)
        .paint_at(ui, poster);
    if resp.hovered() {
        ui.painter().rect_stroke(poster, 12.0 * s, Stroke::new(2.0_f32, ACCENT));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        *hover = Some(item.clone());
    }
    let mut y = poster.bottom() + 8.0 * s;
    paint_wrapped(ui, rect, pos2(rect.left() + 2.0, y), &item.display_title(), m.title_font.clone(), Color32::WHITE, m.pw - 4.0, 2);
    y += m.title_h + 3.0 * s;
    let mut sub = match (item.production_year, item.community_rating) {
        (Some(yr), Some(r)) => format!("{yr}  ·  ★ {r:.1}"),
        (Some(yr), None) => yr.to_string(),
        (None, Some(r)) => format!("★ {r:.1}"),
        _ => String::new(),
    };
    if item.kind == "Series" {
        if let Some(n) = crate::jellyfin::ep_count(&item.id) {
            let tag = format!("{n} {}", if n == 1 { "ep" } else { "eps" });
            sub = if sub.is_empty() { tag } else { format!("{sub}  ·  {tag}") };
        }
    }
    paint_wrapped(ui, rect, pos2(rect.left() + 2.0, y), &sub, m.sub_font.clone(), MUTED, m.pw - 4.0, 1);
    if resp.clicked() {
        *play = Some(item.clone());
    }
}

/// Poster cards in one horizontally scrolling row.
pub fn poster_row(
    ui: &mut egui::Ui,
    client: &Client,
    items: &[Item],
    s: f32,
    id_salt: &str,
    play: &mut Option<Item>,
    hover: &mut Option<Item>,
) {
    let m = poster_metrics(ui, s);
    hscroll(ui, id_salt, s, |ui| {
        ui.horizontal(|ui| {
            for item in items {
                poster_card(ui, client, item, s, &m, play, hover);
            }
        });
        // room for the (floating) scrollbar so it never overlaps the card text
        ui.add_space(18.0 * s);
    });
}

/// Poster cards wrapped into a grid that fills the available width.
pub fn poster_grid(ui: &mut egui::Ui, client: &Client, items: &[Item], s: f32, play: &mut Option<Item>) {
    let m = poster_metrics(ui, s);
    let mut ignore: Option<Item> = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(14.0 * s, 10.0 * s);
        for item in items {
            poster_card(ui, client, item, s, &m, play, &mut ignore);
        }
    });
}

/// Library tiles ("My Media"): 16:9 artwork with the library name on it.
pub fn library_row(ui: &mut egui::Ui, client: &Client, libs: &[Item], s: f32, open: &mut Option<Item>) {
    let (w, h) = (280.0 * s, 280.0 * s * 9.0 / 16.0);
    let r = 14.0 * s;
    hscroll(ui, "library_row", s, |ui| {
        ui.horizontal(|ui| {
            for lib in libs {
                let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
                if lib.image_tags.contains_key("Primary") {
                    egui::Image::new(client.image_url_w(&lib.id, "Primary", (640.0 * s) as u32)).show_loading_spinner(false).rounding(r).paint_at(ui, rect);
                } else {
                    ui.painter().rect_filled(rect, r, PANEL);
                }
                // darken the lower part so the name stays readable on any artwork
                vgradient(
                    ui.painter(),
                    Rect::from_min_max(pos2(rect.left(), rect.top() + h * 0.4), rect.right_bottom()),
                    Color32::TRANSPARENT,
                    Color32::from_black_alpha(200),
                );
                ui.painter().text(
                    pos2(rect.left() + 16.0 * s, rect.bottom() - 14.0 * s),
                    Align2::LEFT_BOTTOM,
                    &lib.name,
                    FontId::proportional(20.0 * s),
                    Color32::WHITE,
                );
                if resp.hovered() {
                    ui.painter().rect_stroke(rect, r, Stroke::new(2.0_f32, ACCENT));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    *open = Some(lib.clone());
                }
            }
        });
        ui.add_space(18.0 * s);
    });
}

/// "1h 12m left" / "32m left"
fn fmt_remaining(secs: f64) -> String {
    let mins = (secs / 60.0).ceil() as i64;
    if mins <= 0 {
        "Almost done".to_string()
    } else if mins >= 60 {
        format!("{}h {:02}m left", mins / 60, mins % 60)
    } else {
        format!("{mins}m left")
    }
}

/// Continue-watching cards: 16:9 thumbnail with a progress line along the bottom, show title,
/// "S1:E2 · Episode title" and time remaining.
pub fn resume_row(
    ui: &mut egui::Ui,
    client: &Client,
    items: &[Item],
    s: f32,
    id_salt: &str,
    play: &mut Option<Item>,
    hover: &mut Option<Item>,
    watched: &mut Option<Item>,
    open: &mut Option<Item>,
    spoiler: bool,
) {
    let name_font = FontId::proportional(14.0 * s);
    let line_font = FontId::proportional(12.5 * s);
    let (w, h) = (300.0 * s, 300.0 * s * 9.0 / 16.0);
    let r = 12.0 * s;
    let (nh, lh) = (row_height(ui, &name_font), row_height(ui, &line_font));
    let total_h = h + 8.0 * s + nh + 2.0 * s + lh + 2.0 * s + lh + 4.0 * s;

    hscroll(ui, id_salt, s, |ui| {
        ui.horizontal(|ui| {
            for item in items {
                let (rect, resp) = ui.allocate_exact_size(vec2(w, total_h), Sense::click());
                let thumb = Rect::from_min_size(rect.min, vec2(w, h));
                ui.painter().rect_filled(thumb, r, PANEL);
                let thumb_url = client.thumb_url(item, (640.0 * s) as u32);
                if spoiler && item.is_episode() && !item.played() {
                    paint_blurred(ui, thumb_url, thumb, r, FULL_UV, 18.0 * s);
                } else {
                    egui::Image::new(thumb_url).show_loading_spinner(false).rounding(r).paint_at(ui, thumb);
                }

                // watched-progress line along the bottom edge of the thumbnail (resumable items only)
                let resuming = item.resume_seconds() > 1.0;
                if resuming {
                    let bar_h = 5.0 * s;
                    let track = Rect::from_min_max(pos2(thumb.left(), thumb.bottom() - bar_h), thumb.right_bottom());
                    let bottom_round = Rounding { nw: 0.0, ne: 0.0, sw: r, se: r };
                    ui.painter().rect_filled(track, bottom_round, Color32::from_black_alpha(170));
                    let frac = item.progress().max(0.02);
                    let fill = Rect::from_min_max(track.min, pos2(track.left() + track.width() * frac, track.bottom()));
                    let fill_round = Rounding { nw: 0.0, ne: 0.0, sw: r, se: if frac >= 0.98 { r } else { 0.0 } };
                    ui.painter().rect_filled(fill, fill_round, ACCENT);
                }

                // "mark as watched" check, top-right, shown while the tile is hovered
                let cd = 28.0 * s;
                let check = Rect::from_center_size(pos2(thumb.right() - 10.0 * s - cd / 2.0, thumb.top() + 10.0 * s + cd / 2.0), vec2(cd, cd));
                let check_resp = ui.interact(check, ui.id().with(("watched", &item.id, id_salt)), Sense::click());
                if resp.hovered() || check_resp.hovered() {
                    let hot = check_resp.hovered();
                    ui.painter().circle_filled(check.center(), cd / 2.0, if hot { ACCENT } else { Color32::from_black_alpha(170) });
                    let st = Stroke::new(2.2_f32, Color32::WHITE);
                    let c = check.center();
                    ui.painter().add(Shape::line(
                        vec![pos2(c.x - 0.28 * cd, c.y + 0.02 * cd), pos2(c.x - 0.08 * cd, c.y + 0.24 * cd), pos2(c.x + 0.3 * cd, c.y - 0.2 * cd)],
                        st,
                    ));
                }
                if check_resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    check_resp.clone().on_hover_text("Mark as watched");
                }
                if check_resp.clicked() {
                    *watched = Some(item.clone());
                    continue;
                }

                if resp.hovered() {
                    ui.painter().rect_stroke(thumb, r, Stroke::new(2.0_f32, ACCENT));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    *hover = Some(item.clone());
                }

                let x = rect.left() + 2.0;
                let tw = w - 4.0;
                let mut y = thumb.bottom() + 8.0 * s;
                // show / movie title -> its page; episode title -> the episode page
                if link_line(ui, rect, pos2(x, y), &item.display_title(), name_font.clone(), Color32::WHITE, tw, egui::Id::new((id_salt, &item.id, 1))) {
                    *open = Some(if item.is_episode() { item.series_stub() } else { item.clone() });
                }
                y += nh + 2.0 * s;
                if item.is_episode() {
                    if link_line(ui, rect, pos2(x, y), &item.episode_label(), line_font.clone(), Color32::from_gray(205), tw, egui::Id::new((id_salt, &item.id, 2))) {
                        *open = Some(item.clone());
                    }
                } else {
                    let year = item.production_year.map(|v| v.to_string()).unwrap_or_default();
                    paint_wrapped(ui, rect, pos2(x, y), &year, line_font.clone(), Color32::from_gray(205), tw, 1);
                }
                y += lh + 2.0 * s;
                let left = if resuming {
                    item.remaining_seconds().map(fmt_remaining).unwrap_or_default()
                } else {
                    item.runtime_minutes().map(|m| format!("{m}m")).unwrap_or_default()
                };
                paint_wrapped(ui, rect, pos2(x, y), &left, line_font.clone(), MUTED, tw, 1);

                if resp.clicked() {
                    *play = Some(item.clone());
                }
            }
        });
        // room for the (floating) scrollbar so it never overlaps the card text
        ui.add_space(18.0 * s);
    });
}

/// One row of the player's episode list: thumbnail, number + title, runtime and summary.
/// Returns true when clicked.
pub fn episode_row(ui: &mut egui::Ui, client: &Client, ep: &Item, current: bool, width: f32, spoil: &Spoiler) -> bool {
    let (tw, th) = (150.0_f32, 84.0_f32);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, th + 12.0), Sense::click());
    if current {
        ui.painter().rect_filled(rect, 10.0, ACCENT.gamma_multiply(0.28));
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 10.0, Color32::from_white_alpha(16));
    }
    let hide = spoil.on && !current && !ep.played();
    let thumb = Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(tw, th));
    ui.painter().rect_filled(thumb, 8.0, PANEL);
    let url = client.image_url_w(&ep.id, "Primary", 400);
    if hide {
        paint_blurred(ui, url, thumb, 8.0, FULL_UV, 0.0);
    } else {
        egui::Image::new(url).show_loading_spinner(false).rounding(8.0).paint_at(ui, thumb);
    }
    let frac = ep.progress();
    if frac > 0.01 {
        let bar = Rect::from_min_max(pos2(thumb.left(), thumb.bottom() - 4.0), thumb.right_bottom());
        ui.painter().rect_filled(bar, Rounding { nw: 0.0, ne: 0.0, sw: 8.0, se: 8.0 }, Color32::from_black_alpha(170));
        let fill = Rect::from_min_max(bar.min, pos2(bar.left() + bar.width() * frac, bar.bottom()));
        ui.painter().rect_filled(fill, Rounding { nw: 0.0, ne: 0.0, sw: 8.0, se: 0.0 }, ACCENT);
    }

    let x = thumb.right() + 12.0;
    let max_w = (rect.right() - x - 8.0).max(40.0);
    let title_font = FontId::proportional(14.5);
    let small = FontId::proportional(12.0);
    let raw = ep.index_number.map(|n| format!("Episode {n}")).unwrap_or_default();
    let title_hidden = hide && spoil.next_id.as_deref() != Some(ep.id.as_str());
    let (title, number) = if title_hidden {
        (if raw.is_empty() { "Episode".to_string() } else { raw }, String::new())
    } else if ep.name.trim().eq_ignore_ascii_case(&raw) {
        (ep.name.clone(), String::new())
    } else {
        (ep.name.clone(), raw)
    };
    let mut y = thumb.top();
    paint_wrapped(ui, rect, pos2(x, y), &title, title_font.clone(), Color32::WHITE, max_w, 1);
    y += row_height(ui, &title_font) + 1.0;
    if !number.is_empty() {
        paint_wrapped(ui, rect, pos2(x, y), &number, small.clone(), Color32::from_gray(205), max_w, 1);
        y += row_height(ui, &small) + 1.0;
    }
    let mut meta = String::new();
    if let Some(m) = ep.runtime_minutes() {
        meta = format!("{m}m");
    }
    if current {
        meta = if meta.is_empty() { "Now playing".to_string() } else { format!("{meta}  ·  Now playing") };
    }
    paint_wrapped(ui, rect, pos2(x, y), &meta, small.clone(), MUTED, max_w, 1);
    y += row_height(ui, &small) + 2.0;
    if !hide {
        if let Some(o) = &ep.overview {
            paint_wrapped(ui, rect, pos2(x, y), o, small, MUTED, max_w, 2);
        }
    }
    if resp.hovered() && !current {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

// ------------------------------------------------------------------------------ hero

/// Full-width featured preview. The backdrop fills the top of the page and fades into the
/// background; the text block uses fixed boxes (logo, meta, genres, summary, button) so nothing
/// moves when the selected item changes. Returns true if Play was pressed.
pub fn hero(ui: &mut egui::Ui, client: &Client, item: &Item, s: f32, opts: &HeroOpts) -> HeroAction {
    let full_w = ui.available_width();
    let hero_h = (full_w * 0.22).clamp(420.0 * s, 520.0 * s);
    let (rect, _) = ui.allocate_exact_size(vec2(full_w, hero_h), Sense::hover());

    // --- backdrop, cropped to cover the whole hero area ---------------------------------------
    let ep_thumb = item.is_episode() && item.image_tags.contains_key("Primary");
    let has_backdrop = ep_thumb || !item.backdrop_image_tags.is_empty() || !item.parent_backdrop_image_tags.is_empty();
    if has_backdrop {
        let (rect_aspect, img_aspect) = (rect.width() / rect.height(), 16.0_f32 / 9.0);
        let uv = if rect_aspect > img_aspect {
            let vh = img_aspect / rect_aspect;
            let y0 = (1.0 - vh) * 0.3;
            Rect::from_min_max(pos2(0.0, y0), pos2(1.0, y0 + vh))
        } else {
            let vw = rect_aspect / img_aspect;
            let x0 = (1.0 - vw) * 0.5;
            Rect::from_min_max(pos2(x0, 0.0), pos2(x0 + vw, 1.0))
        };
        let hero_url = if ep_thumb {
            client.image_url_w(&item.id, "Primary", (1920.0 * s.max(1.0)) as u32)
        } else {
            client.image_url_w(item.art_id(), "Backdrop", (1920.0 * s.max(1.0)) as u32)
        };
        let img_rect = Rect::from_min_max(pos2(rect.left(), rect.top() + 2.0), pos2(rect.right(), rect.bottom() - 2.0));
        if ep_thumb && opts.blur_thumb {
            paint_blurred(ui, hero_url, img_rect, 0.0, uv, 40.0 * s);
        } else {
            egui::Image::new(hero_url).show_loading_spinner(false).uv(uv).paint_at(ui, img_rect);
        }
    } else {
        ui.painter().rect_filled(rect, 0.0, PANEL);
    }
    // fade the left edge (for text) and the bottom edge (into the page background)
    hgradient(
        ui.painter(),
        Rect::from_min_max(rect.left_top(), pos2(rect.left() + rect.width() * 0.62, rect.bottom())),
        Color32::from_rgba_unmultiplied(0x0e, 0x10, 0x16, 235),
        Color32::TRANSPARENT,
    );
    vgradient(
        ui.painter(),
        Rect::from_min_max(pos2(rect.left(), rect.top() + hero_h * 0.45), pos2(rect.right(), rect.bottom() + 2.0)),
        Color32::TRANSPARENT,
        BG,
    );
    // solid strip under the gradient: hides any sub-pixel seam where the image meets the page
    ui.painter().rect_filled(
        Rect::from_min_max(pos2(rect.left(), rect.bottom() - 3.0), pos2(rect.right(), rect.bottom() + 2.0)),
        0.0,
        BG,
    );

    // --- fixed text block --------------------------------------------------------------------
    let x = rect.left() + 32.0 * s;
    let box_w = (560.0 * s).min(rect.width() * 0.6);
    let mut y = rect.top() + 36.0 * s;

    // logo box: the logo is squeezed/enlarged to fit; without a logo the title is shown instead
    let logo_box = Rect::from_min_size(pos2(x, y), vec2(box_w * 0.75, 110.0 * s));
    let art = item.art_id();
    let has_logo = item.image_tags.contains_key("Logo") || item.parent_logo_image_tag.is_some();
    if has_logo {
        let mut lui = ui.new_child(UiBuilder::new().max_rect(logo_box).layout(Layout::left_to_right(Align::BOTTOM)));
        lui.add(egui::Image::new(client.image_url_w(art, "Logo", 800)).show_loading_spinner(false).fit_to_exact_size(logo_box.size()));
    } else {
        let mut job = egui::text::LayoutJob::simple(item.display_title(), FontId::proportional(36.0 * s), Color32::WHITE, box_w);
        job.wrap.max_rows = 2;
        job.wrap.overflow_character = Some('…');
        let galley = ui.painter().layout_job(job);
        let pos = pos2(x, logo_box.bottom() - galley.size().y);
        let mut sjob = egui::text::LayoutJob::simple(item.display_title(), FontId::proportional(36.0 * s), SHADOW, box_w);
        sjob.wrap.max_rows = 2;
        sjob.wrap.overflow_character = Some('…');
        let sg = ui.painter().layout_job(sjob);
        let clipped = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        clipped.galley(pos + vec2(2.0, 2.0) * s, sg, SHADOW);
        clipped.galley(pos, galley, Color32::WHITE);
    }
    let mut open_title = false;
    if opts.link_title {
        let r = ui.interact(logo_box, egui::Id::new(("hero_title_link", &item.id)), Sense::click());
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        open_title = r.clicked();
    }
    y = logo_box.bottom() + 10.0 * s;

    // native / original title (row is always reserved so nothing shifts)
    if let Some(t) = item.native_title() {
        paint_shadowed(ui, rect, pos2(x, y), t, FontId::proportional(15.0 * s), Color32::from_gray(205), box_w, 1, s);
    }
    y += 24.0 * s;

    // meta line: year, rating, score, runtime
    let meta_rect = Rect::from_min_size(pos2(x, y), vec2(box_w, 24.0 * s));
    let date = if item.is_episode() { item.premiere_date.as_deref().and_then(fmt_date) } else { None };
    let ep_count = if item.kind == "Series" { crate::jellyfin::ep_count(&item.id) } else { None };
    // drawn twice: first as a dark offset shadow, then the real thing
    for shadow in [true, false] {
        let off = if shadow { vec2(2.0, 2.0) * s } else { vec2(0.0, 0.0) };
        let mut mui = ui.new_child(UiBuilder::new().max_rect(meta_rect.translate(off)).layout(Layout::left_to_right(Align::Center)));
        mui.set_clip_rect(meta_rect.translate(off).intersect(ui.clip_rect()));
        let c = |col: Color32| if shadow { SHADOW } else { col };
        if let Some(d) = &date {
            mui.label(RichText::new(d).size(14.5 * s).color(c(Color32::from_gray(215))));
        } else if let Some(yr) = item.production_year {
            mui.label(RichText::new(yr.to_string()).size(14.5 * s).color(c(Color32::from_gray(215))));
        }
        if let Some(r) = &item.official_rating {
            let t = RichText::new(format!(" {r} ")).size(12.5 * s).color(c(Color32::from_gray(235)));
            mui.label(if shadow { t } else { t.background_color(Color32::from_white_alpha(34)) });
        }
        if let Some(sc) = item.community_rating {
            mui.label(RichText::new(format!("★ {sc:.1}")).size(14.5 * s).color(c(Color32::from_rgb(255, 200, 70))));
        }
        if let Some(n) = ep_count {
            mui.label(RichText::new(fmt_eps(n)).size(14.5 * s).color(c(Color32::from_gray(215))));
        }
        if let Some(m) = item.runtime_minutes() {
            let t = if m >= 60 { format!("{}h {:02}m", m / 60, m % 60) } else { format!("{m}m") };
            mui.label(RichText::new(if item.kind == "Series" { format!("{t} / ep") } else { t }).size(14.5 * s).color(c(Color32::from_gray(215))));
        }
    }
    y += 28.0 * s;

    // genres line
    let genres = if item.is_episode() {
        item.episode_label()
    } else {
        item.genres.iter().take(4).cloned().collect::<Vec<_>>().join("  •  ")
    };
    paint_shadowed(ui, rect, pos2(x, y), &genres, FontId::proportional(14.0 * s), Color32::from_gray(190), box_w, 1, s);
    y += 26.0 * s;

    // summary: always a four-row box, ellipsised when longer
    let sum_font = FontId::proportional(14.5 * s);
    let sum_h = row_height(ui, &sum_font) * 4.0;
    if let Some(o) = &item.overview {
        paint_shadowed(ui, rect, pos2(x, y), o, sum_font, Color32::from_gray(222), box_w, 4, s);
    }
    y += sum_h + 16.0 * s;

    // play button: compact, and shows the resume point when there is one
    let resume = item.resume_seconds();
    let base = opts.play_label.clone().unwrap_or_else(|| "Play".to_string());
    let label = if resume > 1.0 { format!("▶  {base} from {}", fmt_time(resume)) } else { format!("▶  {base}") };
    let btn_rect = Rect::from_min_size(pos2(x, y), vec2(box_w, 42.0 * s));
    let mut bui = ui.new_child(UiBuilder::new().max_rect(btn_rect).layout(Layout::left_to_right(Align::Center)));
    let mut act = HeroAction { open_title, ..Default::default() };
    act.play = pill_button(&mut bui, &label, 16.0 * s, 40.0 * s, 22.0 * s, 0.0, 20.0 * s, ACCENT, Color32::WHITE).clicked();
    if opts.watch_button {
        bui.add_space(8.0 * s);
        let d = 40.0 * s;
        let (r, resp) = bui.allocate_exact_size(vec2(d, d), Sense::click());
        let fill = if opts.played { ACCENT } else if resp.hovered() { Color32::from_white_alpha(60) } else { Color32::from_white_alpha(36) };
        bui.painter().circle_filled(r.center(), d / 2.0, fill);
        let st = Stroke::new(2.4_f32, Color32::WHITE);
        let c = r.center();
        bui.painter().add(Shape::line(vec![pos2(c.x - 0.2 * d, c.y + 0.02 * d), pos2(c.x - 0.05 * d, c.y + 0.17 * d), pos2(c.x + 0.22 * d, c.y - 0.15 * d)], st));
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let tip = match (item.kind == "Series", opts.played) {
            (true, true) => "Mark series as unwatched",
            (true, false) => "Mark series as watched",
            (false, true) => "Mark as unwatched",
            (false, false) => "Mark as watched",
        };
        act.toggle_watched = resp.on_hover_text(tip).clicked();
    }
    act
}

// ------------------------------------------------------------------------------ misc

pub fn fmt_time(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

pub fn vgradient(p: &egui::Painter, r: Rect, top: Color32, bottom: Color32) {
    let mut m = egui::Mesh::default();
    m.colored_vertex(r.left_top(), top);
    m.colored_vertex(r.right_top(), top);
    m.colored_vertex(r.right_bottom(), bottom);
    m.colored_vertex(r.left_bottom(), bottom);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(m));
}

pub fn hgradient(p: &egui::Painter, r: Rect, left: Color32, right: Color32) {
    let mut m = egui::Mesh::default();
    m.colored_vertex(r.left_top(), left);
    m.colored_vertex(r.right_top(), right);
    m.colored_vertex(r.right_bottom(), right);
    m.colored_vertex(r.left_bottom(), left);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(m));
}

fn lang_name(code: &str) -> String {
    let c = code.to_ascii_lowercase();
    let name = match c.as_str() {
        "eng" | "en" => "English",
        "jpn" | "ja" => "Japanese",
        "spa" | "es" => "Spanish",
        "fre" | "fra" | "fr" => "French",
        "ger" | "deu" | "de" => "German",
        "ita" | "it" => "Italian",
        "por" | "pt" => "Portuguese",
        "rus" | "ru" => "Russian",
        "kor" | "ko" => "Korean",
        "chi" | "zho" | "zh" => "Chinese",
        "ara" | "ar" => "Arabic",
        "hin" | "hi" => "Hindi",
        "dut" | "nld" | "nl" => "Dutch",
        "pol" | "pl" => "Polish",
        "swe" | "sv" => "Swedish",
        "tur" | "tr" => "Turkish",
        "tha" | "th" => "Thai",
        "vie" | "vi" => "Vietnamese",
        "und" | "" => return String::new(),
        _ => return code.to_uppercase(),
    };
    name.to_string()
}

/// "English · SDH (SUBRIP) [Forced]"
pub fn track_label(t: &Track) -> String {
    let lang = lang_name(&t.lang);
    let mut label = if lang.is_empty() { format!("Track {}", t.id) } else { lang.clone() };
    if !t.title.is_empty() && !t.title.eq_ignore_ascii_case(&lang) {
        label.push_str(&format!(" · {}", t.title));
    }
    if !t.codec.is_empty() {
        label.push_str(&format!(" ({})", t.codec.to_uppercase()));
    }
    if t.forced {
        label.push_str(" [Forced]");
    }
    label
}

/// Pill-shaped volume slider filled with the accent colour. `value` is 0..=max.
/// Returns true when the user changed it; `active` is set while hovered or dragged.
pub fn volume_slider(ui: &mut egui::Ui, value: &mut f64, max: f64, width: f32) -> (bool, egui::Response) {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 24.0), Sense::click_and_drag());
    let mut changed = false;
    if resp.dragged() || resp.clicked() || resp.is_pointer_button_down_on() {
        if let Some(p) = resp.interact_pointer_pos() {
            let f = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
            *value = (f * max).round();
            changed = true;
        }
    }
    let track = Rect::from_center_size(rect.center(), vec2(width, 10.0));
    ui.painter().rect_filled(track, 5.0, Color32::from_white_alpha(55));
    let f = (*value / max).clamp(0.0, 1.0) as f32;
    if f > 0.0 {
        let fill = Rect::from_min_size(track.min, vec2((track.width() * f).max(10.0), track.height()));
        ui.painter().rect_filled(fill, 5.0, ACCENT);
    }
    (changed, resp)
}

// ------------------------------------------------------------------------------ title page widgets

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TileMode {
    /// Episode of the open show: episode title, "Episode N".
    Episode,
    /// Extras: title only.
    Extra,
    /// Search result episode: show name, "S1:E2 · Title".
    Search,
}

/// Horizontal 16:9 tiles (episodes, extras, search hits). Click plays.
pub fn tile_row(
    ui: &mut egui::Ui,
    client: &Client,
    items: &[Item],
    s: f32,
    id_salt: &str,
    mode: TileMode,
    play: &mut Option<Item>,
    hover: &mut Option<Item>,
    open: &mut Option<Item>,
    watched: &mut Option<Item>,
    spoil: &Spoiler,
) {
    let name_font = FontId::proportional(14.0 * s);
    let line_font = FontId::proportional(12.5 * s);
    let (w, h) = (270.0 * s, 270.0 * s * 9.0 / 16.0);
    let r = 12.0 * s;
    let (nh, lh) = (row_height(ui, &name_font), row_height(ui, &line_font));
    let total_h = h + 8.0 * s + nh + 2.0 * s + lh + 2.0 * s + lh + 4.0 * s;

    hscroll(ui, id_salt, s, |ui| {
        ui.horizontal(|ui| {
            for item in items {
                let (rect, resp) = ui.allocate_exact_size(vec2(w, total_h), Sense::click());
                let thumb = Rect::from_min_size(rect.min, vec2(w, h));
                ui.painter().rect_filled(thumb, r, PANEL);
                // spoiler control: unwatched episodes get a heavily down-scaled (= blurred) thumbnail
                let hide = spoil.on && mode == TileMode::Episode && !item.played();
                let url = if mode == TileMode::Extra && !item.image_tags.contains_key("Primary") {
                    None
                } else {
                    Some(client.thumb_url(item, (600.0 * s) as u32))
                };
                if let Some(url) = url {
                    if hide {
                        paint_blurred(ui, url, thumb, r, FULL_UV, 18.0 * s);
                    } else {
                        egui::Image::new(url).show_loading_spinner(false).rounding(r).paint_at(ui, thumb);
                    }
                }
                let resuming = item.resume_seconds() > 1.0;
                if resuming {
                    let bar_h = 5.0 * s;
                    let track = Rect::from_min_max(pos2(thumb.left(), thumb.bottom() - bar_h), thumb.right_bottom());
                    let rd = Rounding { nw: 0.0, ne: 0.0, sw: r, se: r };
                    ui.painter().rect_filled(track, rd, Color32::from_black_alpha(170));
                    let frac = item.progress().max(0.02);
                    let fill = Rect::from_min_max(track.min, pos2(track.left() + track.width() * frac, track.bottom()));
                    ui.painter().rect_filled(fill, Rounding { nw: 0.0, ne: 0.0, sw: r, se: if frac >= 0.98 { r } else { 0.0 } }, ACCENT);
                }
                // watched toggle (episode lists): filled when watched, otherwise shown on hover
                if mode == TileMode::Episode {
                    let cd = 28.0 * s;
                    let check = Rect::from_center_size(pos2(thumb.right() - 10.0 * s - cd / 2.0, thumb.top() + 10.0 * s + cd / 2.0), vec2(cd, cd));
                    let cr = ui.interact(check, egui::Id::new((id_salt, "watched", &item.id)), Sense::click());
                    let played = item.played();
                    if resp.hovered() || cr.hovered() || played {
                        let fill = if played || cr.hovered() { ACCENT } else { Color32::from_black_alpha(170) };
                        ui.painter().circle_filled(check.center(), cd / 2.0, fill);
                        let c = check.center();
                        ui.painter().add(Shape::line(
                            vec![pos2(c.x - 0.28 * cd, c.y + 0.02 * cd), pos2(c.x - 0.08 * cd, c.y + 0.24 * cd), pos2(c.x + 0.3 * cd, c.y - 0.2 * cd)],
                            Stroke::new(2.2_f32, Color32::WHITE),
                        ));
                    }
                    if cr.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if cr.clone().on_hover_text(if played { "Mark as unwatched" } else { "Mark as watched" }).clicked() {
                        *watched = Some(item.clone());
                        continue;
                    }
                }
                if resp.hovered() {
                    ui.painter().rect_stroke(thumb, r, Stroke::new(2.0_f32, ACCENT));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    *hover = Some(item.clone());
                }
                let (title, second) = match mode {
                    TileMode::Episode => {
                        let raw = item.index_number.map(|n| format!("Episode {n}")).unwrap_or_default();
                        if hide && spoil.next_id.as_deref() != Some(item.id.as_str()) {
                            // title hidden: the number takes the title line (not repeated below)
                            (if raw.is_empty() { "Episode".to_string() } else { raw }, String::new())
                        } else if item.name.trim().eq_ignore_ascii_case(&raw) {
                            (item.name.clone(), String::new())
                        } else {
                            (item.name.clone(), raw)
                        }
                    }
                    TileMode::Extra => (item.name.clone(), String::new()),
                    TileMode::Search => (item.display_title(), if item.is_episode() { item.episode_label() } else { String::new() }),
                };
                let mut third = if resuming {
                    item.remaining_seconds().map(fmt_remaining).unwrap_or_default()
                } else {
                    item.runtime_minutes().filter(|m| *m > 0).map(|m| format!("{m}m")).unwrap_or_default()
                };
                if mode == TileMode::Episode {
                    if let Some(d) = item.premiere_date.as_deref().and_then(fmt_date) {
                        third = if third.is_empty() { d } else { format!("{third}  ·  {d}") };
                    }
                }
                let (x, tw) = (rect.left() + 2.0, w - 4.0);
                let mut y = thumb.bottom() + 8.0 * s;
                let target1 = match mode {
                    TileMode::Episode => Some(item.clone()),
                    TileMode::Search if item.is_episode() => Some(item.series_stub()),
                    TileMode::Search => Some(item.clone()),
                    TileMode::Extra => None,
                };
                let id1 = egui::Id::new((id_salt, &item.id, 1));
                if let Some(t) = target1 {
                    if link_line(ui, rect, pos2(x, y), &title, name_font.clone(), Color32::WHITE, tw, id1) {
                        *open = Some(t);
                    }
                } else {
                    paint_wrapped(ui, rect, pos2(x, y), &title, name_font.clone(), Color32::WHITE, tw, 1);
                }
                y += nh + 2.0 * s;
                if mode == TileMode::Search && item.is_episode() {
                    if link_line(ui, rect, pos2(x, y), &second, line_font.clone(), Color32::from_gray(205), tw, egui::Id::new((id_salt, &item.id, 2))) {
                        *open = Some(item.clone());
                    }
                } else {
                    paint_wrapped(ui, rect, pos2(x, y), &second, line_font.clone(), Color32::from_gray(205), tw, 1);
                }
                y += lh + 2.0 * s;
                paint_wrapped(ui, rect, pos2(x, y), &third, line_font.clone(), MUTED, tw, 1);
                if resp.clicked() {
                    *play = Some(item.clone());
                }
            }
        });
        ui.add_space(18.0 * s);
    });
}

/// Round portraits with name and role (cast, people search). Click opens the person.
pub fn people_row(
    ui: &mut egui::Ui,
    client: &Client,
    people: &[crate::jellyfin::Person],
    s: f32,
    id_salt: &str,
    open: &mut Option<crate::jellyfin::Person>,
) {
    let d = 112.0 * s;
    let name_font = FontId::proportional(13.5 * s);
    let role_font = FontId::proportional(12.0 * s);
    let (nh, rh) = (row_height(ui, &name_font) * 2.0, row_height(ui, &role_font));
    let cell_w = d + 24.0 * s;
    hscroll(ui, id_salt, s, |ui| {
        ui.horizontal(|ui| {
            for p in people {
                let (rect, resp) = ui.allocate_exact_size(vec2(cell_w, d + 8.0 * s + nh + rh + 6.0 * s), Sense::click());
                let circle = Rect::from_center_size(pos2(rect.center().x, rect.top() + d / 2.0), vec2(d, d));
                ui.painter().circle_filled(circle.center(), d / 2.0, PANEL);
                if p.primary_image_tag.is_some() {
                    let img = egui::Image::new(client.image_url(&p.id, "Primary", (d * 2.0) as u32)).show_loading_spinner(false);
                    // crop to a square (keeping the top of portraits) instead of stretching
                    let uv = match img.load_for_size(ui.ctx(), vec2(d, d)) {
                        Ok(egui::load::TexturePoll::Ready { texture }) if texture.size.x > 0.0 && texture.size.y > 0.0 => {
                            let aspect = texture.size.x / texture.size.y;
                            if aspect < 1.0 {
                                let y0 = (1.0 - aspect) * 0.12;
                                Rect::from_min_max(pos2(0.0, y0), pos2(1.0, y0 + aspect))
                            } else {
                                let x0 = (1.0 - 1.0 / aspect) / 2.0;
                                Rect::from_min_max(pos2(x0, 0.0), pos2(x0 + 1.0 / aspect, 1.0))
                            }
                        }
                        _ => Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    };
                    img.uv(uv).rounding(d / 2.0).paint_at(ui, circle);
                } else {
                    let initial = p.name.chars().next().map(|c| c.to_string()).unwrap_or_default();
                    ui.painter().text(circle.center(), Align2::CENTER_CENTER, initial, FontId::proportional(d * 0.4), MUTED);
                }
                if resp.hovered() {
                    ui.painter().circle_stroke(circle.center(), d / 2.0, Stroke::new(2.0_f32, ACCENT));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let tx = rect.left() + 2.0;
                let y = circle.bottom() + 8.0 * s;
                paint_wrapped(ui, rect, pos2(tx, y), &p.name, name_font.clone(), Color32::WHITE, cell_w - 4.0, 2);
                let role = p.role.as_deref().filter(|r| !r.is_empty()).or(Some(p.kind.as_str()).filter(|k| !matches!(*k, "" | "Actor" | "Person")));
                if let Some(role) = role {
                    paint_wrapped(ui, rect, pos2(tx, y + nh + 2.0 * s), role, role_font.clone(), MUTED, cell_w - 4.0, 1);
                }
                if resp.clicked() {
                    *open = Some(p.clone());
                }
            }
        });
        ui.add_space(18.0 * s);
    });
}

/// A line of text that acts as a link (accent colour + hand cursor on hover). Returns true when clicked.
pub fn link_line(ui: &mut egui::Ui, clip: Rect, pos: Pos2, text: &str, font: FontId, color: Color32, w: f32, id: egui::Id) -> bool {
    if text.is_empty() {
        return false;
    }
    let text_w = ui.fonts(|f| f.layout_no_wrap(text.to_owned(), font.clone(), color).size().x).min(w);
    let r = Rect::from_min_size(pos, vec2(text_w, row_height(ui, &font)));
    let resp = ui.interact(r.intersect(clip), id, Sense::click());
    let hot = resp.hovered();
    if hot {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    paint_wrapped(ui, clip, pos, text, font, if hot { Color32::from_rgb(0xa9, 0xb8, 0xff) } else { color }, w, 1);
    resp.clicked()
}

/// "2024-10-05T00:00:00Z" -> "October 5, 2024"
pub fn fmt_date(iso: &str) -> Option<String> {
    let mut p = iso.get(..10)?.split('-');
    let (y, m, d): (u32, usize, u32) = (p.next()?.parse().ok()?, p.next()?.parse().ok()?, p.next()?.parse().ok()?);
    const M: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    Some(format!("{} {d}, {y}", M.get(m.checked_sub(1)?)?))
}

pub fn fmt_eps(n: u32) -> String {
    if n == 1 { "1 episode".into() } else { format!("{n} episodes") }
}

const SHADOW: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 190);

/// Text with a soft dark drop shadow (used on top of artwork).
#[allow(clippy::too_many_arguments)]
pub fn paint_shadowed(ui: &egui::Ui, clip: Rect, pos: Pos2, text: &str, font: FontId, color: Color32, max_w: f32, max_rows: usize, s: f32) {
    paint_wrapped(ui, clip, pos + vec2(1.0, 1.0) * s, text, font.clone(), Color32::from_black_alpha(90), max_w, max_rows);
    paint_wrapped(ui, clip, pos + vec2(2.0, 2.0) * s, text, font.clone(), SHADOW, max_w, max_rows);
    paint_wrapped(ui, clip, pos, text, font, color, max_w, max_rows);
}

/// Dropdown-style button with a painted arrow (the font has no arrow glyph). `size` is the text size.
pub fn dropdown_button(ui: &mut egui::Ui, text: &str, size: f32) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), FontId::proportional(size), Color32::WHITE);
    let (pad, tri) = (size * 0.8, size * 0.55);
    let (w, h) = (galley.size().x + pad * 2.0 + tri + size * 0.5, galley.size().y + size * 0.7);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    let fill = if resp.hovered() { Color32::from_rgb(0x33, 0x38, 0x48) } else { Color32::from_rgb(0x24, 0x28, 0x34) };
    ui.painter().rect_filled(rect, 10.0, fill);
    ui.painter().galley(pos2(rect.left() + pad, rect.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
    let c = pos2(rect.right() - pad - tri / 2.0, rect.center().y);
    ui.painter().add(Shape::convex_polygon(
        vec![c + vec2(-tri / 2.0, -tri * 0.25), c + vec2(tri / 2.0, -tri * 0.25), c + vec2(0.0, tri * 0.3)],
        Color32::from_gray(220),
        Stroke::NONE,
    ));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Horizontal row that also scrolls with the mouse wheel (vertical wheel = sideways) while the
/// pointer is over it. At either end the wheel falls through to scroll the page again.
pub fn hscroll(ui: &mut egui::Ui, id_salt: &str, s: f32, add: impl FnOnce(&mut egui::Ui)) {
    let out = egui::ScrollArea::horizontal().id_salt(id_salt).show(ui, |ui| add(ui));
    let max = (out.content_size.x - out.inner_rect.width()).max(0.0);
    if max <= 1.0 {
        return;
    }
    // directional nudge buttons (dragging the row still works too)
    let view = out.inner_rect;
    let off = out.state.offset.x;
    let d = 34.0 * s;
    let y = view.top() + (out.content_size.y - 18.0 * s).max(0.0) * 0.36;
    for (dir, show) in [(-1.0_f32, off > 1.0), (1.0, off < max - 1.0)] {
        if !show {
            continue;
        }
        let x = if dir < 0.0 { view.left() + 6.0 * s + d / 2.0 } else { view.right() - 6.0 * s - d / 2.0 };
        let r = Rect::from_center_size(pos2(x, y), vec2(d, d));
        let resp = ui.interact(r.intersect(ui.clip_rect()), egui::Id::new((id_salt, "nudge", dir as i32)), Sense::click());
        let fill = if resp.hovered() { Color32::from_black_alpha(220) } else { Color32::from_black_alpha(150) };
        ui.painter().circle_filled(r.center(), d / 2.0, fill);
        let st = Stroke::new(2.4_f32, Color32::WHITE);
        let (c, w, h) = (r.center(), 0.09 * d, 0.2 * d);
        let tip = c.x + dir * w;
        ui.painter().add(Shape::line(vec![pos2(c.x - dir * w, c.y - h), pos2(tip, c.y), pos2(c.x - dir * w, c.y + h)], st));
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            let mut st = out.state;
            st.offset.x = (off + dir * view.width() * 0.8).clamp(0.0, max);
            st.store(ui.ctx(), out.id);
            ui.ctx().request_repaint();
        }
    }
}

enum Slot {
    Pending,
    Failed,
    Image(egui::ColorImage),
    Tex(egui::TextureHandle),
}

fn blurs() -> &'static std::sync::Mutex<HashMap<String, Slot>> {
    static B: std::sync::OnceLock<std::sync::Mutex<HashMap<String, Slot>>> = std::sync::OnceLock::new();
    B.get_or_init(Default::default)
}

fn fetch_blurred(url: &str) -> Option<egui::ColorImage> {
    let bytes = reqwest::blocking::get(url).ok()?.error_for_status().ok()?.bytes().ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let small = img.resize(256, 256, image::imageops::FilterType::Triangle).to_rgba8();
    let blurred = image::imageops::blur(&small, 7.0);
    Some(egui::ColorImage::from_rgba_unmultiplied([blurred.width() as usize, blurred.height() as usize], blurred.as_raw()))
}

/// Spoiler blur: the image is fetched once, shrunk and Gaussian-blurred on a worker thread, then
/// stretched smoothly over `rect`. Until it is ready only a dark placeholder is drawn (nothing leaks).
pub fn paint_blurred(ui: &egui::Ui, url: String, rect: Rect, rounding: f32, base_uv: Rect, _radius: f32) {
    let ctx = ui.ctx().clone();
    let id = {
        let Ok(mut m) = blurs().lock() else { return };
        match m.get_mut(&url) {
            None => {
                m.insert(url.clone(), Slot::Pending);
                let (u2, c2) = (url.clone(), ctx.clone());
                std::thread::spawn(move || {
                    let slot = fetch_blurred(&u2).map_or(Slot::Failed, Slot::Image);
                    if let Ok(mut m) = blurs().lock() {
                        m.insert(u2, slot);
                    }
                    c2.request_repaint();
                });
                None
            }
            Some(slot) => match slot {
                Slot::Image(img) => {
                    let tex = ctx.load_texture(format!("blur:{url}"), img.clone(), egui::TextureOptions::LINEAR);
                    let id = tex.id();
                    *slot = Slot::Tex(tex);
                    Some(id)
                }
                Slot::Tex(t) => Some(t.id()),
                _ => None,
            },
        }
    };
    match id {
        Some(id) => {
            egui::Image::new(egui::load::SizedTexture::new(id, rect.size())).uv(base_uv).rounding(rounding).paint_at(ui, rect);
        }
        None => {
            ui.painter().rect_filled(rect, rounding, PANEL);
        }
    }
}

const FULL_UV: Rect = Rect { min: Pos2 { x: 0.0, y: 0.0 }, max: Pos2 { x: 1.0, y: 1.0 } };

pub fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>().trim_end())
    }
}

/// Spoiler-control settings handed to the episode rows.
#[derive(Clone, Default)]
pub struct Spoiler {
    pub on: bool,
    /// Episode that is next to watch (the only unwatched one whose title is shown).
    pub next_id: Option<String>,
}

#[derive(Default)]
pub struct HeroOpts {
    /// Replaces "Play" (e.g. "Play S1:E1").
    pub play_label: Option<String>,
    /// Show the watched-toggle check next to Play.
    pub watch_button: bool,
    pub played: bool,
    /// Blur the episode thumbnail (spoiler control).
    pub blur_thumb: bool,
    /// Clicking the logo / title opens the show or movie page.
    pub link_title: bool,
}

#[derive(Default)]
pub struct HeroAction {
    pub play: bool,
    pub toggle_watched: bool,
    pub open_title: bool,
}
