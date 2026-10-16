//! Calendar tab: upcoming episodes from Sonarr, one month at a time.

use super::{App, Msg};
use crate::sonarr::CalEntry;
use crate::ui::{ACCENT, MUTED, PANEL};
use chrono::{Datelike, Duration as Days, Local, Months, NaiveDate};
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Stroke};

pub(super) struct CalState {
    pub month: NaiveDate,
    pub entries: Vec<CalEntry>,
    pub loading: bool,
    pub error: Option<String>,
    pub req: u64,
}

impl CalState {
    pub fn new() -> Self {
        Self { month: first_of(Local::now().date_naive()), entries: vec![], loading: false, error: None, req: 0 }
    }
}

fn first_of(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

/// First and last day shown for a month (whole weeks, Sunday first)
fn grid_range(month: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = month - Days::days(month.weekday().num_days_from_sunday() as i64);
    let next = month.checked_add_months(Months::new(1)).unwrap_or(month);
    let days = (next - month).num_days() + month.weekday().num_days_from_sunday() as i64;
    let weeks = (days + 6) / 7;
    (start, start + Days::days(weeks * 7 - 1))
}

/// Shortens `text` with "…" until it fits `max_w`
fn fit(ui: &egui::Ui, text: &str, font: &FontId, max_w: f32) -> String {
    let width = |t: &str| ui.fonts(|f| f.layout_no_wrap(t.to_string(), font.clone(), Color32::WHITE).size().x);
    if width(text) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len();
    while n > 1 {
        n -= 1;
        let t: String = chars[..n].iter().collect::<String>().trim_end().to_string() + "…";
        if width(&t) <= max_w {
            return t;
        }
    }
    "…".to_string()
}

impl App {
    fn sonarr_ready(&self) -> bool {
        !self.cfg.sonarr_url.trim().is_empty() && !self.cfg.sonarr_key.trim().is_empty()
    }

    /// Called when the tab is opened: jump to the current month and load it
    pub(super) fn calendar_open(&mut self, ctx: &egui::Context) {
        self.cal.month = first_of(Local::now().date_naive());
        self.calendar_fetch(ctx);
    }

    fn calendar_fetch(&mut self, ctx: &egui::Context) {
        if !self.sonarr_ready() {
            return;
        }
        self.cal.req += 1;
        self.cal.loading = true;
        self.cal.error = None;
        let (req, (start, end)) = (self.cal.req, grid_range(self.cal.month));
        let (url, key, tx, ctx) = (self.cfg.sonarr_url.clone(), self.cfg.sonarr_key.clone(), self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = crate::sonarr::calendar(&url, &key, start, end);
            let _ = tx.send(Msg::Calendar { req, result });
            ctx.request_repaint();
        });
    }

    pub(super) fn on_calendar(&mut self, req: u64, result: Result<Vec<CalEntry>, String>) {
        if req != self.cal.req {
            return;
        }
        self.cal.loading = false;
        match result {
            Ok(e) => self.cal.entries = e,
            Err(e) => {
                log::warn!("sonarr calendar failed: {e}");
                self.cal.entries.clear();
                self.cal.error = Some(e);
            }
        }
    }

    pub(super) fn calendar_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, s: f32) {
        let mut shift: i32 = 0;
        let mut today = false;
        let mut refresh = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Calendar").size(24.0 * s).strong());
            ui.add_space(16.0 * s);
            if ui.button("‹").clicked() {
                shift = -1;
            }
            ui.label(RichText::new(self.cal.month.format("%B %Y").to_string()).size(18.0 * s).strong());
            if ui.button("›").clicked() {
                shift = 1;
            }
            if ui.button("Today").clicked() {
                today = true;
            }
            if self.cal.loading {
                ui.add(egui::Spinner::new().size(18.0 * s));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                refresh = crate::ui::icon_button(ui, crate::ui::Icon::Refresh, 30.0 * s, !self.cal.loading).on_hover_text("Refresh").clicked();
            });
        });
        ui.add_space(8.0 * s);
        if today {
            self.cal.month = first_of(Local::now().date_naive());
        } else if shift != 0 {
            let m = self.cal.month;
            self.cal.month = if shift < 0 { m.checked_sub_months(Months::new(1)) } else { m.checked_add_months(Months::new(1)) }.unwrap_or(m);
        }
        if today || shift != 0 || refresh {
            self.calendar_fetch(ctx);
        }

        if !self.sonarr_ready() {
            ui.label(RichText::new("Add your Sonarr address and API key in Settings to see upcoming releases here.").color(MUTED));
            return;
        }
        if let Some(e) = &self.cal.error {
            ui.colored_label(Color32::LIGHT_RED, format!("Could not load the calendar: {e}"));
        }

        let month = self.cal.month;
        let (start, end) = grid_range(month);
        let weeks = ((end - start).num_days() + 1) / 7;
        let today_date = Local::now().date_naive();
        let col_w = ui.available_width() / 7.0;
        let (name_font, sub_font) = (FontId::proportional(12.5 * s), FontId::proportional(11.0 * s));
        let entry_h = 34.0 * s;
        let head_h = 26.0 * s;

        egui::ScrollArea::vertical().id_salt("cal_scroll").auto_shrink([false, false]).show(ui, |ui| {
            // weekday header
            let (hdr, _) = ui.allocate_exact_size(vec2(col_w * 7.0, 22.0 * s), Sense::hover());
            for (i, d) in ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"].iter().enumerate() {
                ui.painter().text(pos2(hdr.left() + col_w * (i as f32 + 0.5), hdr.center().y), Align2::CENTER_CENTER, d, FontId::proportional(12.5 * s), MUTED);
            }
            for w in 0..weeks {
                let days: Vec<NaiveDate> = (0..7).map(|d| start + Days::days(w * 7 + d)).collect();
                let per_day: Vec<Vec<&CalEntry>> = days.iter().map(|d| self.cal.entries.iter().filter(|e| e.date == *d).collect()).collect();
                let most = per_day.iter().map(|v| v.len()).max().unwrap_or(0) as f32;
                let row_h = (head_h + most * entry_h + 8.0 * s).max(86.0 * s);
                let (row, _) = ui.allocate_exact_size(vec2(col_w * 7.0, row_h), Sense::hover());
                for (i, day) in days.iter().enumerate() {
                    let cell = Rect::from_min_size(pos2(row.left() + col_w * i as f32, row.top()), vec2(col_w, row_h)).shrink(2.0 * s);
                    let in_month = day.month() == month.month();
                    let is_today = *day == today_date;
                    ui.painter().rect_filled(cell, 8.0 * s, if in_month { PANEL } else { PANEL.gamma_multiply(0.5) });
                    if is_today {
                        ui.painter().rect_stroke(cell, 8.0 * s, Stroke::new(1.5_f32, ACCENT));
                    }
                    let num_col = if is_today { Color32::WHITE } else if in_month { Color32::from_gray(210) } else { MUTED.gamma_multiply(0.6) };
                    let label = if day.day() == 1 { day.format("%b %-d").to_string() } else { day.day().to_string() };
                    ui.painter().text(cell.left_top() + vec2(8.0 * s, 6.0 * s), Align2::LEFT_TOP, label, FontId::proportional(12.5 * s), num_col);
                    for (n, e) in per_day[i].iter().enumerate() {
                        let top = cell.top() + head_h + n as f32 * entry_h;
                        let r = Rect::from_min_size(pos2(cell.left() + 4.0 * s, top), vec2(cell.width() - 8.0 * s, entry_h - 2.0 * s));
                        let code = format!("S{}:E{}", e.season, e.episode);
                        let line2 = match e.time {
                            Some(t) => format!("{code} · {}", t.format("%-I:%M %p")),
                            None => code,
                        };
                        let resp = ui.interact(r, egui::Id::new(("cal_entry", *day, n)), Sense::hover());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, 5.0 * s, Color32::from_white_alpha(14));
                        }
                        let max_w = r.width() - 8.0 * s;
                        ui.painter().text(r.left_top() + vec2(4.0 * s, 2.0 * s), Align2::LEFT_TOP, fit(ui, &e.show, &name_font, max_w), name_font.clone(), Color32::from_gray(235));
                        ui.painter().text(r.left_top() + vec2(4.0 * s, 17.0 * s), Align2::LEFT_TOP, fit(ui, &line2, &sub_font, max_w), sub_font.clone(), MUTED);
                        resp.on_hover_text(format!("{}\n{line2}", e.show));
                    }
                }
            }
        });
    }
}
