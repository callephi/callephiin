//! Search tab, media pages and person pages.

use super::{App, Msg, Tab};
use crate::jellyfin::{Item, Person, SearchResults, TitleData};
use crate::ui::{self, TileMode, MUTED};
use eframe::egui::{self, vec2, Color32, RichText};
use std::time::{Duration, Instant};

/// Decides what list is show in the main row
#[derive(Clone, PartialEq)]
pub(super) enum Sel {
    Season(String),
    /// Series-level extras when more than one season
    Extras,
}

pub(super) struct TitleView {
    pub item: Item,
    pub seasons: Vec<Item>,
    pub sel: Sel,
    pub episodes: Vec<Item>,
    pub series_extras: Vec<Item>,
    pub season_extras: Vec<Item>,
    pub loading: bool,
    pub ep_loading: bool,
    pub error: Option<String>,
    /// Episode last hovered: its summary is shown in the hero
    pub hero_ep: Option<Item>,
    pub similar: Vec<Item>,
    pub next_ep: Option<Item>,
    /// Season list open
    pub menu: bool,
    /// Episode ID the row should be lined up with once it is drawn
    pub focus: Option<String>,
    pub req: u64,
}

pub(super) struct PersonView {
    pub person: Person,
    pub items: Vec<Item>,
    pub loading: bool,
    pub req: u64,
}

pub(super) enum Detail {
    Title(TitleView),
    Person(PersonView),
}

#[derive(Default)]
pub(super) struct SearchState {
    pub query: String,
    pub results: SearchResults,
    pub dirty_at: Option<Instant>,
    pub loading: bool,
    pub req: u64,
    pub focus: bool,
    /// The query `results` belongs to
    pub done_for: String,
}

fn pretty_ep(item: &Item, single_season: bool) -> String {
    match (single_season, item.index_number) {
        (true, Some(n)) if item.name.is_empty() => format!("Episode {n}"),
        (true, Some(n)) => format!("Episode {n} · {}", item.name),
        _ => item.episode_label(),
    }
}

impl App {
    // ---------------------------------------------------------------- opening pages

    pub(super) fn open_title(&mut self, item: Item, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else { return };
        self.detail_req += 1;
        let req = self.detail_req;
        self.detail.push(Detail::Title(TitleView {
            item: item.clone(),
            seasons: vec![],
            sel: Sel::Extras,
            episodes: vec![],
            series_extras: vec![],
            season_extras: vec![],
            loading: true,
            ep_loading: false,
            error: None,
            hero_ep: None,
            similar: vec![],
            next_ep: None,
            menu: false,
            focus: None,
            req,
        }));
        self.scroll_reset = true;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = client.load_title(&item).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Title { req, result });
            ctx.request_repaint();
        });
    }

    pub(super) fn open_person(&mut self, person: Person, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else { return };
        self.detail_req += 1;
        let req = self.detail_req;
        let id = person.id.clone();
        self.detail.push(Detail::Person(PersonView { person, items: vec![], loading: true, req }));
        self.scroll_reset = true;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let items = client.person_titles(&id);
            let _ = tx.send(Msg::Person { req, items });
            ctx.request_repaint();
        });
    }

    fn pick_season(&mut self, sel: Sel, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else { return };
        let Some(Detail::Title(v)) = self.detail.last_mut() else { return };
        v.sel = sel.clone();
        v.hero_ep = None;
        v.menu = false;
        let Sel::Season(season_id) = sel else {
            v.ep_loading = false;
            return;
        };
        v.ep_loading = true;
        v.episodes.clear();
        v.season_extras.clear();
        let (req, series_id, has_logo) = (v.req, v.item.id.clone(), v.item.image_tags.contains_key("Logo"));
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let episodes = client.season_episodes(&series_id, &season_id);
            let extras = client.extras(&season_id, &series_id, has_logo);
            let _ = tx.send(Msg::TitleSeason { req, season_id, episodes, extras });
            ctx.request_repaint();
        });
    }

    pub(super) fn on_title_loaded(&mut self, req: u64, result: Result<TitleData, String>) {
        for d in self.detail.iter_mut() {
            if let Detail::Title(v) = d {
                if v.req != req {
                    continue;
                }
                v.loading = false;
                match &result {
                    Ok(t) => {
                        v.item = t.item.clone();
                        v.similar = t.similar.clone();
                        v.next_ep = t.next_ep.clone();
                        v.seasons = t.seasons.clone();
                        v.episodes = t.episodes.clone();
                        v.series_extras = t.series_extras.clone();
                        v.season_extras = t.season_extras.clone();
                        v.focus = t.focus.clone();
                        if let Some(id) = &t.season_id {
                            v.sel = Sel::Season(id.clone());
                        }
                    }
                    Err(e) => v.error = Some(e.clone()),
                }
            }
        }
    }

    pub(super) fn on_title_season(&mut self, req: u64, season_id: String, episodes: Vec<Item>, extras: Vec<Item>) {
        for d in self.detail.iter_mut() {
            if let Detail::Title(v) = d {
                if v.req == req && v.sel == Sel::Season(season_id.clone()) {
                    v.episodes = episodes.clone();
                    v.season_extras = extras.clone();
                    v.ep_loading = false;
                }
            }
        }
    }

    pub(super) fn on_title_refresh(&mut self, req: u64, item: Item, next: Option<Item>, episodes: Vec<Item>) {
        for d in self.detail.iter_mut() {
            if let Detail::Title(v) = d {
                if v.req == req {
                    let people = std::mem::take(&mut v.item.people);
                    v.item = item.clone();
                    if v.item.people.is_empty() {
                        v.item.people = people;
                    }
                    v.next_ep = next.clone();
                    if !episodes.is_empty() {
                        v.episodes = episodes.clone();
                    }
                }
            }
        }
    }

    /// Mark the open media watched or unwatched (a whole series at once)
    fn toggle_favorite(&mut self, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else { return };
        let Some(Detail::Title(v)) = self.detail.last_mut() else { return };
        let ud = v.item.user_data.get_or_insert_with(Default::default);
        ud.is_favorite = !ud.is_favorite;
        let (id, fav, ctx) = (v.item.id.clone(), ud.is_favorite, ctx.clone());
        std::thread::spawn(move || {
            let _ = client.set_favorite(&id, fav);
            ctx.request_repaint();
        });
    }

    fn toggle_watched(&mut self, ctx: &egui::Context) {
        let Some(Detail::Title(v)) = self.detail.last() else { return };
        let target = v.item.clone();
        self.toggle_item_watched(target, ctx);
    }

    /// Mark a movie / show / episode of the open page watched or unwatched, then refresh the page.
    fn toggle_item_watched(&mut self, target: Item, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else { return };
        let Some(Detail::Title(v)) = self.detail.last_mut() else { return };
        let played = !target.played();
        // optimistic local update
        for it in std::iter::once(&mut v.item).chain(v.episodes.iter_mut()) {
            if it.id == target.id {
                let ud = it.user_data.get_or_insert_with(Default::default);
                ud.played = played;
                ud.playback_position_ticks = 0;
            }
        }
        let (req, page_id) = (v.req, v.item.id.clone());
        let series = if v.item.kind == "Episode" { v.item.series_id.clone() } else { None };
        let season = match (&v.sel, v.item.kind.as_str()) {
            (_, "Episode") => v.item.season_id.clone(),
            (Sel::Season(s), _) => Some(s.clone()),
            _ => None,
        };
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = if played { client.mark_played(&target.id) } else { client.mark_unplayed(&target.id) };
            if let Ok((item, next, episodes)) = client.refresh_title(&page_id, series.as_deref(), season.as_deref()) {
                let _ = tx.send(Msg::TitleRefresh { req, item, next, episodes });
            }
            ctx.request_repaint();
        });
    }

    pub(super) fn on_person_loaded(&mut self, req: u64, items: Vec<Item>) {
        for d in self.detail.iter_mut() {
            if let Detail::Person(v) = d {
                if v.req == req {
                    v.items = items.clone();
                    v.loading = false;
                }
            }
        }
    }

    // ---------------------------------------------------------------- title page

    pub(super) fn detail_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, s: f32) {
        match self.detail.last() {
            Some(Detail::Title(_)) => self.title_ui(ui, ctx, s),
            Some(Detail::Person(_)) => self.person_ui(ui, ctx, s),
            None => {}
        }
    }

    fn title_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, s: f32) {
        let Some(client) = self.client.clone() else { return };
        let Some(Detail::Title(v)) = self.detail.last() else { return };
        let (item, seasons, sel) = (v.item.clone(), v.seasons.clone(), v.sel.clone());
        let (episodes, series_extras, season_extras) = (v.episodes.clone(), v.series_extras.clone(), v.season_extras.clone());
        let (loading, ep_loading, error, hero_ep) = (v.loading, v.ep_loading, v.error.clone(), v.hero_ep.clone());
        let (similar, menu) = (v.similar.clone(), v.menu);
        let focus = v.focus.clone();
        let mut focus_used = false;
        let mut toggle_menu = false;
        let mut close_menu = false;
        let is_series = item.kind == "Series";

        // categories: real seasons in order, then Specials (season 0), then series extras
        let mut regular: Vec<&Item> = seasons.iter().filter(|x| x.index_number != Some(0)).collect();
        regular.sort_by_key(|x| x.index_number.unwrap_or(u32::MAX));
        let specials: Vec<&Item> = seasons.iter().filter(|x| x.index_number == Some(0)).collect();
        let cats = regular.len() + specials.len();
        let multi = is_series && cats > 1;
        let single_season = is_series && !multi;
        let one_real = regular.len() <= 1;

        let (mut back, mut play, mut open, mut open_p, mut pick) =
            (false, None::<Item>, None::<Item>, None::<Person>, None::<Sel>);
        let mut hover: Option<Item> = None;
        let mut hero_play = false;
        let mut hero_toggle = false;
        let mut hero_fav = false;
        let mut watched_ep: Option<Item> = None;
        let next_ep = match self.detail.last() {
            Some(Detail::Title(v)) => v.next_ep.clone(),
            _ => None,
        };
        let spoil = ui::Spoiler { on: self.cfg.spoiler_control, next_id: next_ep.as_ref().map(|e| e.id.clone()) };
        const HIDDEN: &str = "(This episode's synopsis is currently hidden to prevent spoilers.)";

        let mut area = egui::ScrollArea::vertical().id_salt("title_scroll").auto_shrink([false, false]);
        if std::mem::take(&mut self.scroll_reset) {
            area = area.vertical_scroll_offset(0.0);
        }
        area.show(ui, |ui| {
            if ui.button("<  Back").clicked() {
                back = true;
            }
            ui.add_space(8.0 * s);

            let mut hero_item = item.clone();
            let mut opts = ui::HeroOpts { watch_button: true, played: item.played(), favorite: Some(item.user_data.as_ref().map_or(false, |u| u.is_favorite)), link_title: item.kind == "Episode", ..Default::default() };
            if is_series {
                opts.play_label = next_ep.as_ref().and_then(|e| e.ep_code()).map(|c| format!("Play {c}"));
                // an episode in progress turns the button into "Resume S1:E10 from 10:01"
                opts.resume = next_ep.as_ref().map(|e| e.resume_seconds());
            }
            if let Some(ep) = &hero_ep {
                let hide = spoil.on && !ep.played();
                let mut shown = ep.clone();
                if hide && spoil.next_id.as_deref() != Some(ep.id.as_str()) {
                    shown.name.clear();
                }
                let head = pretty_ep(&shown, single_season);
                let body = if hide { Some(HIDDEN) } else { ep.overview.as_deref() };
                hero_item.overview = Some(match body {
                    Some(o) if !o.is_empty() => format!("{head}\n{o}"),
                    _ => head,
                });
            }
            if item.kind == "Episode" && spoil.on && !item.played() {
                opts.blur_thumb = true;
                hero_item.overview = Some(HIDDEN.to_string());
                if spoil.next_id.as_deref() != Some(item.id.as_str()) {
                    hero_item.name.clear();
                }
            }
            let act = ui::hero(ui, &client, &hero_item, s, &opts);
            hero_play = act.play;
            hero_toggle = act.toggle_watched;
            hero_fav = act.toggle_favorite;
            if act.open_title {
                open = Some(item.series_stub());
            }
            ui.add_space(14.0 * s);

            if let Some(e) = &error {
                ui.colored_label(Color32::LIGHT_RED, format!("Could not load this page: {e}"));
            }

            if is_series {
                // ---- season picker / heading
                let cur_name = match &sel {
                    Sel::Season(id) => seasons.iter().find(|x| &x.id == id).map(|x| x.name.clone()).unwrap_or_default(),
                    Sel::Extras => "Extras".to_string(),
                };
                if multi {
                    let btn = ui::dropdown_button(ui, &ui::ellipsize(&cur_name, 70), 17.0 * s);
                    if btn.clicked() {
                        toggle_menu = true;
                    }
                    if menu {
                        // floating list (does not push the page down); five rows visible, scrolls beyond that
                        let row_h = 28.0 * s;
                        let gap = 2.0 * s;
                        let area = egui::Area::new("season_popup".into())
                            .order(egui::Order::Foreground)
                            .fixed_pos(btn.rect.left_bottom() + vec2(0.0, 4.0 * s))
                            .show(ui.ctx(), |ui| {
                                egui::Frame::none()
                                    .fill(Color32::from_rgb(28, 31, 42))
                                    .rounding(10.0 * s)
                                    .inner_margin(6.0 * s)
                                    .shadow(egui::epaint::Shadow { offset: vec2(0.0, 4.0), blur: 14.0, spread: 0.0, color: Color32::from_black_alpha(140) })
                                    .show(ui, |ui| {
                                        // as wide as the longest (ellipsized) name needs, no more
                                        let mut names: Vec<String> = regular.iter().chain(specials.iter()).map(|x| ui::ellipsize(&x.name, 70)).collect();
                                        names.push("Extras".to_string());
                                        let text_w = names
                                            .iter()
                                            .map(|n| ui.fonts(|f| f.layout_no_wrap(n.clone(), egui::FontId::proportional(14.0 * s), Color32::WHITE).size().x))
                                            .fold(0.0_f32, f32::max);
                                        ui.set_width(text_w + 56.0 * s);
                                        ui.spacing_mut().item_spacing.y = gap;
                                        let rows = (regular.len() + specials.len() + usize::from(!one_real && !series_extras.is_empty())).clamp(1, 8) as f32;
                                        let list_h = rows * row_h + (rows - 1.0) * gap;
                                        egui::ScrollArea::vertical()
                                            .id_salt("title_season_scroll")
                                            .min_scrolled_height(list_h)
                                            .max_height(list_h)
                                            .auto_shrink([false, false])
                                            .show(ui, |ui| {
                                                for x in regular.iter().chain(specials.iter()) {
                                                    let w = ui.available_width();
                                                    if ui.add_sized(vec2(w, row_h), egui::SelectableLabel::new(sel == Sel::Season(x.id.clone()), ui::ellipsize(&x.name, 70))).clicked() {
                                                        pick = Some(Sel::Season(x.id.clone()));
                                                    }
                                                }
                                                if !one_real && !series_extras.is_empty() {
                                                    let w = ui.available_width();
                                                    if ui.add_sized(vec2(w, row_h), egui::SelectableLabel::new(sel == Sel::Extras, "Extras")).clicked() {
                                                        pick = Some(Sel::Extras);
                                                    }
                                                }
                                            });
                                    });
                            });
                        let clicked_elsewhere = ui.input(|i| i.pointer.any_pressed())
                            && ui.input(|i| i.pointer.interact_pos()).map_or(false, |p| !area.response.rect.contains(p) && !btn.rect.contains(p));
                        if clicked_elsewhere {
                            close_menu = true;
                        }
                    }
                } else {
                    ui.label(RichText::new("Episodes").size(18.0 * s).strong());
                }
                ui.add_space(8.0 * s);

                if cats == 0 {
                    if loading {
                        ui.spinner();
                    } else {
                        ui.label(RichText::new("No episodes are currently available.").color(MUTED));
                    }
                } else if sel == Sel::Extras {
                    ui::tile_row(ui, &client, &series_extras, s, "title_extras_main", TileMode::Extra, &mut play, &mut hover, &mut open, &mut watched_ep, &spoil);
                } else if loading || ep_loading {
                    ui.spinner();
                } else if episodes.is_empty() {
                    ui.label(RichText::new("No episodes are currently available.").color(MUTED));
                } else {
                    if let Some(f) = focus.as_ref().filter(|f| episodes.iter().any(|e| &e.id == *f)) {
                        ui.data_mut(|d| d.insert_temp(egui::Id::new("scroll_to_ep"), f.clone()));
                        focus_used = true;
                    }
                    ui::tile_row(ui, &client, &episodes, s, &format!("title_eps_{cur_name}"), TileMode::Episode, &mut play, &mut hover, &mut open, &mut watched_ep, &spoil);
                }
                ui.add_space(14.0 * s);

                // ---- extras of the selected season (or all extras for single-season shows)
                let mut extras: Vec<Item> = Vec::new();
                if one_real {
                    // one real season: extras are always shown, whatever is picked (Season 1 / Specials)
                    extras = series_extras.clone();
                    for e in &season_extras {
                        if !extras.iter().any(|x| x.id == e.id) {
                            extras.push(e.clone());
                        }
                    }
                } else if sel != Sel::Extras {
                    extras = season_extras.clone();
                }
                if !extras.is_empty() {
                    ui.label(RichText::new("Extras").size(18.0 * s).strong());
                    ui.add_space(8.0 * s);
                    ui::tile_row(ui, &client, &extras, s, "title_extras", TileMode::Extra, &mut play, &mut hover, &mut open, &mut watched_ep, &spoil);
                    ui.add_space(14.0 * s);
                }
            } else if item.kind != "Episode" {
                // ---- movie: extras
                if loading {
                    ui.spinner();
                }
                if !series_extras.is_empty() {
                    ui.label(RichText::new("Extras").size(18.0 * s).strong());
                    ui.add_space(8.0 * s);
                    ui::tile_row(ui, &client, &series_extras, s, "title_extras", TileMode::Extra, &mut play, &mut hover, &mut open, &mut watched_ep, &spoil);
                    ui.add_space(14.0 * s);
                }
            } else {
                // episode page: the other episodes of its season
                if loading {
                    ui.spinner();
                }
                if !episodes.is_empty() {
                    ui.label(RichText::new("Episodes").size(18.0 * s).strong());
                    ui.add_space(8.0 * s);
                    ui::tile_row(ui, &client, &episodes, s, "title_eps_ep", TileMode::Episode, &mut play, &mut hover, &mut open, &mut watched_ep, &spoil);
                    ui.add_space(14.0 * s);
                }
            }

            // ---- cast
            let cast: Vec<Person> = item.people.iter().filter(|x| matches!(x.kind.as_str(), "Actor" | "GuestStar")).cloned().collect();
            if !cast.is_empty() {
                ui.label(RichText::new("Cast").size(18.0 * s).strong());
                ui.add_space(8.0 * s);
                ui::people_row(ui, &client, &cast, s, "title_cast", &mut open_p);
                ui.add_space(14.0 * s);
            }

            // ---- staff & production (text only)
            let lines = staff_lines(&item);
            if !lines.is_empty() {
                ui.label(RichText::new("Staff & Production").size(18.0 * s).strong());
                ui.add_space(8.0 * s);
                ui.scope(|ui| {
                    ui.set_max_width(860.0 * s);
                    for (k, v) in &lines {
                        ui.horizontal_top(|ui| {
                            ui.add_sized(vec2(150.0 * s, 20.0 * s), egui::Label::new(RichText::new(k).size(14.0 * s).color(MUTED)));
                            ui.add(egui::Label::new(RichText::new(v).size(14.0 * s)).wrap());
                        });
                    }
                });
                ui.add_space(14.0 * s);
            }

            // ---- recommendations
            if !similar.is_empty() {
                ui.label(RichText::new("More like this").size(18.0 * s).strong());
                ui.add_space(8.0 * s);
                let mut ignore: Option<Item> = None;
                ui::poster_row(ui, &client, &similar, s, "title_similar", &mut open, &mut ignore);
                ui.add_space(14.0 * s);
            }
            ui.add_space(20.0 * s);
        });

        if back {
            self.detail.pop();
            return;
        }
        if close_menu {
            if let Some(Detail::Title(v)) = self.detail.last_mut() {
                v.menu = false;
            }
        }
        if toggle_menu {
            if let Some(Detail::Title(v)) = self.detail.last_mut() {
                v.menu = !v.menu;
            }
        }
        // the hovered episode's summary shows in the hero only while it is hovered
        if is_series {
            if let Some(Detail::Title(v)) = self.detail.last_mut() {
                v.hero_ep = hover.filter(|h| h.is_episode());
            }
        }
        if let Some(sel) = pick {
            self.pick_season(sel, ctx);
        }
        if let Some(p) = open_p {
            self.open_person(p, ctx);
        }
        if let Some(i) = open {
            self.open_title(i, ctx);
        }
        if let Some(ep) = watched_ep {
            self.toggle_item_watched(ep, ctx);
        }
        if focus_used {
            if let Some(Detail::Title(v)) = self.detail.last_mut() {
                v.focus = None;
            }
        }
        if hero_fav {
            self.toggle_favorite(ctx);
        }
        if hero_toggle {
            self.toggle_watched(ctx);
        }
        if hero_play {
            self.start_play(item.clone(), ctx);
        } else if let Some(i) = play {
            self.start_play(i, ctx);
        }
    }

    fn person_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, s: f32) {
        let Some(client) = self.client.clone() else { return };
        let Some(Detail::Person(v)) = self.detail.last() else { return };
        let (name, items, loading) = (v.person.name.clone(), v.items.clone(), v.loading);
        let (mut back, mut open) = (false, None::<Item>);
        egui::ScrollArea::vertical().id_salt("person_scroll").auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("<  Back").clicked() {
                    back = true;
                }
                ui.label(RichText::new(&name).size(26.0 * s).strong());
            });
            ui.add_space(12.0 * s);
            if loading {
                ui.spinner();
            } else if items.is_empty() {
                ui.label(RichText::new("Nothing in your libraries.").color(MUTED));
            }
            ui::poster_grid(ui, &client, &items, s, &mut open);
            ui.add_space(20.0 * s);
        });
        if back {
            self.detail.pop();
        } else if let Some(i) = open {
            self.open_title(i, ctx);
        }
    }

    // ---------------------------------------------------------------- search

    pub(super) fn search_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, s: f32) {
        let Some(client) = self.client.clone() else { return };
        ui.label(RichText::new("Search").size(26.0 * s).strong());
        ui.add_space(10.0 * s);

        let edit = egui::TextEdit::singleline(&mut self.search.query)
            .hint_text(RichText::new("Movies, shows, episodes, people…").color(Color32::from_gray(85)))
            .font(egui::FontId::proportional(18.0 * s))
            .desired_width(520.0 * s)
            .margin(egui::vec2(10.0 * s, 8.0 * s));
        let resp = ui.add(edit);
        if std::mem::take(&mut self.search.focus) {
            resp.request_focus();
        }
        if resp.changed() {
            self.search.dirty_at = Some(Instant::now());
        }
        if let Some(t) = self.search.dirty_at {
            let wait = Duration::from_millis(280);
            if t.elapsed() >= wait {
                self.search.dirty_at = None;
                self.run_search(ctx);
            } else {
                ctx.request_repaint_after(wait - t.elapsed());
            }
        }
        ui.add_space(14.0 * s);

        let (mut play, mut open, mut open_p) = (None::<Item>, None::<Item>, None::<Person>);
        let mut ignore_w: Option<Item> = None;
        let mut hover: Option<Item> = None;
        let r = self.search.results.clone();
        let q = self.search.query.trim().to_string();

        egui::ScrollArea::vertical().id_salt("search_scroll").auto_shrink([false, false]).show(ui, |ui| {
            if !r.titles.is_empty() {
                ui.label(RichText::new("Movies & shows").size(18.0 * s).strong());
                ui.add_space(8.0 * s);
                ui::poster_row(ui, &client, &r.titles, s, "search_titles", &mut open, &mut hover);
                ui.add_space(14.0 * s);
            }
            if !r.episodes.is_empty() {
                ui.label(RichText::new("Episodes").size(18.0 * s).strong());
                ui.add_space(8.0 * s);
                ui::tile_row(ui, &client, &r.episodes, s, "search_eps", TileMode::Search, &mut play, &mut hover, &mut open, &mut ignore_w, &ui::Spoiler::default());
                ui.add_space(14.0 * s);
            }
            if !r.people.is_empty() {
                ui.label(RichText::new("People").size(18.0 * s).strong());
                ui.add_space(8.0 * s);
                ui::people_row(ui, &client, &r.people, s, "search_people", &mut open_p);
                ui.add_space(14.0 * s);
            }
            if self.search.loading {
                ui.spinner();
            } else if !q.is_empty()
                && self.search.done_for == q
                && r.titles.is_empty()
                && r.episodes.is_empty()
                && r.people.is_empty()
            {
                ui.label(RichText::new("No results.").color(MUTED));
            }
        });

        if let Some(p) = open_p {
            self.open_person(p, ctx);
        } else if let Some(i) = open {
            self.open_title(i, ctx);
        } else if let Some(i) = play {
            self.start_play(i, ctx);
        }
    }

    fn run_search(&mut self, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else { return };
        let q = self.search.query.trim().to_string();
        self.search.req += 1;
        if q.is_empty() {
            self.search.results = SearchResults::default();
            self.search.loading = false;
            self.search.done_for.clear();
            return;
        }
        self.search.loading = true;
        let req = self.search.req;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let results = client.search(&q);
            let _ = tx.send(Msg::Search { req, query: q, results });
            ctx.request_repaint();
        });
    }

    pub(super) fn on_search(&mut self, req: u64, query: String, results: SearchResults) {
        if req == self.search.req {
            self.search.results = results;
            self.search.loading = false;
            self.search.done_for = query;
        }
    }

    pub(super) fn go_tab(&mut self, tab: Tab) {
        self.detail.clear();
        if tab == Tab::Search {
            self.search.focus = true;
        }
        // every tab click lands on that tab's top level (also Home while inside a library or show)
        self.library = None;
        self.scroll_reset = true;
        self.tab = tab;
    }
}

/// "Director: A, B" style rows from the credits plus studios and production locations
fn staff_lines(item: &Item) -> Vec<(String, String)> {
    const ORDER: [&str; 8] = ["Director", "Writer", "Producer", "Creator", "Composer", "Editor", "Conductor", "Lyricist"];
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for p in item.people.iter().filter(|p| !matches!(p.kind.as_str(), "Actor" | "GuestStar" | "")) {
        match groups.iter_mut().find(|(k, _)| *k == p.kind) {
            Some((_, v)) => {
                if !v.contains(&p.name) {
                    v.push(p.name.clone());
                }
            }
            None => groups.push((p.kind.clone(), vec![p.name.clone()])),
        }
    }
    groups.sort_by_key(|(k, _)| ORDER.iter().position(|o| o == k).unwrap_or(ORDER.len()));
    let mut out: Vec<(String, String)> = groups
        .into_iter()
        .map(|(k, v)| (if v.len() > 1 { format!("{k}s") } else { k }, v.join(", ")))
        .collect();
    if !item.studios.is_empty() {
        let names: Vec<_> = item.studios.iter().map(|s| s.name.clone()).collect();
        out.push((if names.len() > 1 { "Studios" } else { "Studio" }.to_string(), names.join(", ")));
    }
    if !item.production_locations.is_empty() {
        out.push(("Production location".to_string(), item.production_locations.join(", ")));
    }
    out
}
