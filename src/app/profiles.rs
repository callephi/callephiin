//! Profiles: switcher screen, sign-in form, sidebar avatar

use super::{App, Tab};
use crate::config::Profile;
use crate::jellyfin::Client;
use crate::ui::{self, ACCENT, BG, MUTED, PANEL};
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Stroke};

#[derive(PartialEq, Clone, Copy)]
pub(super) enum View {
    Normal,
    Switcher,
    /// Sign-in form to add a profile or sign in
    Add,
}

const PALETTE: [(u8, u8, u8); 8] = [
    (0x5b, 0x7c, 0xff),
    (0xe0, 0x6c, 0x75),
    (0x56, 0xb6, 0xa0),
    (0xd1, 0x9a, 0x4a),
    (0xa0, 0x7b, 0xe0),
    (0x4f, 0xa8, 0xd8),
    (0xd2, 0x6f, 0xb0),
    (0x7f, 0xb0, 0x5a),
];

fn color_for(seed: &str) -> Color32 {
    let h = seed.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let (r, g, b) = PALETTE[h as usize % PALETTE.len()];
    Color32::from_rgb(r, g, b)
}

fn paint_avatar(p: &egui::Painter, c: egui::Pos2, d: f32, name: &str, seed: &str) {
    p.circle_filled(c, d / 2.0, color_for(seed));
    let initial = name.chars().next().map(|ch| ch.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    p.text(c, Align2::CENTER_CENTER, initial, FontId::proportional(d * 0.46), Color32::WHITE);
}

/// Round profile button for the bottom of the sidebar
pub(super) fn sidebar_avatar(ui: &mut egui::Ui, profile: Option<&Profile>, d: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(d, d), Sense::click());
    let (name, seed) = profile.map(|p| (p.display_name(), p.id.clone())).unwrap_or(("?".into(), "?".into()));
    paint_avatar(ui.painter(), rect.center(), d, &name, &seed);
    if resp.hovered() {
        ui.painter().circle_stroke(rect.center(), d / 2.0 + 2.0, Stroke::new(2.0_f32, Color32::WHITE));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(name)
}

impl App {
    /// Forgets everything cached from previous account
    pub(super) fn reset_session(&mut self) {
        self.admin = false;
        self.server_version.clear();
        self.latest.clear();
        self.resume.clear();
        self.next_up.clear();
        self.libraries.clear();
        self.library = None;
        self.detail.clear();
        self.selected = None;
        self.search = Default::default();
        self.tab = Tab::Home;
    }

    pub(super) fn activate_profile(&mut self, id: &str, ctx: &egui::Context) {
        if !self.cfg.activate(id) {
            return;
        }
        self.cfg.save();
        self.reset_session();
        self.view = View::Normal;
        self.manage = false;
        if self.cfg.logged_in() {
            self.client = Some(Client::from_config(&self.cfg));
            self.status.clear();
            self.refresh_home(ctx);
        } else {
            // token was cleared (signed out / expired): ask for the password again
            self.client = None;
            self.open_sign_in(self.cfg.server_url.clone(), self.cfg.username.clone());
            self.status = "Please sign in again.".into();
        }
    }

    pub(super) fn open_sign_in(&mut self, url: String, user: String) {
        self.form_url = url;
        self.form_user = user;
        self.password.clear();
        self.new_name.clear();
        self.view = View::Add;
    }

    /// Sign the active account out (but its profile stays in the list)
    pub(super) fn sign_out_active(&mut self) {
        self.cfg.sign_out();
        self.cfg.save();
        self.client = None;
        self.reset_session();
        self.view = if self.cfg.profiles.is_empty() { View::Add } else { View::Switcher };
    }

    pub(super) fn remove_profile(&mut self, id: &str) {
        let was_active = self.cfg.last_profile == id;
        self.cfg.profiles.retain(|p| p.id != id);
        if was_active {
            self.cfg.sign_out();
            self.cfg.last_profile.clear();
            self.client = None;
            self.reset_session();
        }
        self.cfg.save();
        if self.cfg.profiles.is_empty() {
            self.manage = false;
            self.view = View::Add;
        }
    }

    pub(super) fn switcher_ui(&mut self, ctx: &egui::Context, s: f32) {
        let mut pick: Option<String> = None;
        let mut remove: Option<String> = None;
        let mut add = false;
        let mut close = false;
        egui::CentralPanel::default().frame(egui::Frame::none().fill(BG)).show(ctx, |ui| {
            ui::scale_style(ui, s);
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.2).max(20.0));
                ui::logo(ui, 64.0 * s);
                ui.add_space(8.0 * s);
                ui.label(RichText::new(if self.manage { "Manage accounts" } else { "Who's watching?" }).size(30.0 * s).strong());
                ui.add_space(28.0 * s);

                let (card_w, gap, d) = (150.0 * s, 18.0 * s, 104.0 * s);
                let n = self.cfg.profiles.len() + 1;
                let total = n as f32 * card_w + (n as f32 - 1.0) * gap;
                let card_h = d + 70.0 * s;
                ui.allocate_ui_with_layout(vec2(total, card_h), Layout::left_to_right(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for p in &self.cfg.profiles {
                        let (rect, resp) = ui.allocate_exact_size(vec2(card_w, card_h), Sense::click());
                        let c = pos2(rect.center().x, rect.top() + d / 2.0);
                        paint_avatar(ui.painter(), c, d, &p.display_name(), &p.id);
                        if resp.hovered() && !self.manage {
                            ui.painter().circle_stroke(c, d / 2.0 + 3.0, Stroke::new(2.5_f32, Color32::WHITE));
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        let active = self.client.is_some() && p.id == self.cfg.last_profile;
                        ui::paint_wrapped(ui, rect, pos2(rect.left(), rect.top() + d + 10.0 * s), &ui::ellipsize(&p.display_name(), 16), FontId::proportional(16.0 * s), Color32::WHITE, card_w, 1);
                        let sub = if active { format!("{} · signed in", p.host()) } else { p.host() };
                        ui::paint_wrapped(ui, rect, pos2(rect.left(), rect.top() + d + 34.0 * s), &ui::ellipsize(&sub, 24), FontId::proportional(12.0 * s), MUTED, card_w, 1);
                        if self.manage {
                            let b = Rect::from_center_size(pos2(c.x + d * 0.36, c.y - d * 0.36), vec2(30.0 * s, 30.0 * s));
                            let br = ui.interact(b, egui::Id::new(("rm", &p.id)), Sense::click());
                            ui.painter().circle_filled(b.center(), 15.0 * s, if br.hovered() { Color32::from_rgb(0xd9, 0x3b, 0x3b) } else { Color32::from_rgb(0x8a, 0x2a, 0x2a) });
                            let st = Stroke::new(2.2_f32, Color32::WHITE);
                            let k = 5.0 * s;
                            ui.painter().line_segment([b.center() + vec2(-k, -k), b.center() + vec2(k, k)], st);
                            ui.painter().line_segment([b.center() + vec2(-k, k), b.center() + vec2(k, -k)], st);
                            if br.on_hover_text("Remove this profile").clicked() {
                                remove = Some(p.id.clone());
                            }
                        } else if resp.clicked() {
                            pick = Some(p.id.clone());
                        }
                    }
                    if !self.manage {
                        // "+" card to the right of all profiles
                        let (rect, resp) = ui.allocate_exact_size(vec2(card_w, card_h), Sense::click());
                        let c = pos2(rect.center().x, rect.top() + d / 2.0);
                        let hot = resp.hovered();
                        ui.painter().circle_filled(c, d / 2.0, if hot { Color32::from_white_alpha(28) } else { PANEL });
                        let st = Stroke::new(3.0_f32, if hot { Color32::WHITE } else { MUTED });
                        ui.painter().line_segment([c + vec2(-d * 0.16, 0.0), c + vec2(d * 0.16, 0.0)], st);
                        ui.painter().line_segment([c + vec2(0.0, -d * 0.16), c + vec2(0.0, d * 0.16)], st);
                        ui::paint_wrapped(ui, rect, pos2(rect.left(), rect.top() + d + 10.0 * s), "Add profile", FontId::proportional(16.0 * s), MUTED, card_w, 1);
                        if hot {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if resp.clicked() {
                            add = true;
                        }
                    }
                });

                ui.add_space(26.0 * s);
                ui.horizontal(|ui| {
                    ui.add_space(((ui.available_width() - 160.0 * s) / 2.0).max(0.0));
                    if ui::pill_button(ui, if self.manage { "Done" } else { "Manage accounts" }, 15.0 * s, 38.0 * s, 20.0 * s, 150.0 * s, 19.0 * s, Color32::from_rgb(0x24, 0x28, 0x34), Color32::WHITE).clicked() {
                        self.manage = !self.manage;
                    }
                });
            });
        });
        if self.client.is_some() && !self.manage && corner_cross(ctx, s) {
            close = true;
        }
        if let Some(id) = remove {
            self.remove_profile(&id);
        }
        if let Some(id) = pick {
            self.activate_profile(&id, ctx);
        }
        if add {
            self.open_sign_in(String::new(), String::new());
        }
        if close {
            self.view = View::Normal;
        }
    }

    pub(super) fn login_ui(&mut self, ctx: &egui::Context, s: f32) {
        let faint = |t: &str| RichText::new(t.to_owned()).color(Color32::from_gray(85));
        let mut back = false;
        egui::CentralPanel::default().frame(egui::Frame::none().fill(BG)).show(ctx, |ui| {
            ui::scale_style(ui, s);
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.14).max(16.0));
                let w = 360.0 * s;
                egui::Frame::none().fill(PANEL).rounding(20.0 * s).inner_margin(egui::Margin::symmetric(32.0 * s, 28.0 * s)).show(ui, |ui| {
                    ui.set_width(w);
                    ui.vertical_centered(|ui| {
                        ui::logo(ui, 72.0 * s);
                        ui.add_space(6.0 * s);
                        ui.label(RichText::new(if self.cfg.profiles.is_empty() { "Connect to Jellyfin" } else { "Add a profile" }).size(24.0 * s).strong());
                        ui.label(RichText::new("Sign in to your server").size(13.0 * s).color(MUTED));
                        ui.add_space(18.0 * s);

                        let edit = |ui: &mut egui::Ui, text: &mut String, hint: RichText, pw: bool| {
                            ui.add(
                                egui::TextEdit::singleline(text)
                                    .hint_text(hint)
                                    .password(pw)
                                    .font(FontId::proportional(15.0 * s))
                                    .margin(vec2(12.0 * s, 9.0 * s))
                                    .desired_width(w),
                            )
                        };
                        edit(ui, &mut self.form_url, faint("Server address, e.g. http://192.168.1.10:8096"), false);
                        ui.add_space(8.0 * s);
                        edit(ui, &mut self.form_user, faint("Username"), false);
                        ui.add_space(8.0 * s);
                        let pw = edit(ui, &mut self.password, faint("Password"), true);
                        ui.add_space(8.0 * s);
                        edit(ui, &mut self.new_name, faint("Profile name (optional)"), false);
                        ui.add_space(18.0 * s);
                        let enter = pw.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter));
                        let go = ui::pill_button(ui, if self.busy { "Signing in…" } else { "Sign in" }, 16.0 * s, 42.0 * s, 20.0 * s, w, 21.0 * s, ACCENT, Color32::WHITE);
                        if (go.clicked() || enter) && !self.busy {
                            self.start_login(ctx);
                        }
                        if !self.status.is_empty() {
                            ui.add_space(10.0 * s);
                            ui.label(RichText::new(&self.status).color(MUTED));
                        }
                        if let Some(e) = &self.player_error {
                            ui.add_space(10.0 * s);
                            ui.colored_label(Color32::LIGHT_RED, format!("libmpv: {e}"));
                        }
                    });
                });
            });
        });
        if !self.cfg.profiles.is_empty() {
            back = corner_cross(ctx, s);
        }
        if back {
            self.status.clear();
            self.view = View::Switcher;
        }
    }
}

/// Exit button for profile switcher
fn corner_cross(ctx: &egui::Context, s: f32) -> bool {
    let mut clicked = false;
    egui::Area::new("corner_close".into()).order(egui::Order::Foreground).fixed_pos(pos2(ctx.screen_rect().right() - 70.0 * s, 24.0 * s)).show(ctx, |ui| {
        let (r, resp) = ui.allocate_exact_size(vec2(44.0 * s, 44.0 * s), Sense::click());
        if resp.hovered() {
            ui.painter().circle_filled(r.center(), 22.0 * s, Color32::from_white_alpha(28));
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let st = Stroke::new(2.4_f32, if resp.hovered() { Color32::WHITE } else { MUTED });
        let k = 7.0 * s;
        ui.painter().line_segment([r.center() + vec2(-k, -k), r.center() + vec2(k, k)], st);
        ui.painter().line_segment([r.center() + vec2(-k, k), r.center() + vec2(k, -k)], st);
        clicked = resp.clicked();
    });
    clicked
}
