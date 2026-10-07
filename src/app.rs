mod pages;
mod profiles;
use profiles::View;
use pages::{Detail, SearchState};
use crate::config::{Config, SeriesTracks, StoredTrack, StreamMode};
use crate::discord::{self, NowPlaying};
use crate::jellyfin::{Client, Item, SearchResults, TitleData, PlayInfo, Segment, SegmentKind, SortKey, PAGE_SIZES};
use crate::player::{Player, Track, TrackKind};
use crate::ui::{self, Icon, SeekEvent, SeekMark, ACCENT, BG, MUTED, PANEL};
use eframe::egui::{self, pos2, vec2, Align, Color32, Layout, Rect, RichText, Rounding, Sense, UiBuilder};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Home,
    Search,
    Settings,
}

/// What the "skip" button currently does.
#[derive(Clone, Copy)]
enum Skip {
    Intro(f64),
    Next,
}

enum Msg {
    Login(Result<Config, String>),
    Home { latest: Vec<Item>, resume: Vec<Item>, next_up: Vec<Item>, libraries: Vec<Item>, admin: bool, version: String },
    /// One page of a library (`req` identifies the request so stale answers are dropped).
    Library { req: u64, result: Result<(Vec<Item>, usize), String> },
    Title { req: u64, result: Result<TitleData, String> },
    TitleSeason { req: u64, season_id: String, episodes: Vec<Item>, extras: Vec<Item> },
    Person { req: u64, items: Vec<Item> },
    TitleRefresh { req: u64, item: Item, next: Option<Item>, episodes: Vec<Item> },
    Search { req: u64, query: String, results: SearchResults },
    Error(String),
    Play(Result<PlayInfo, String>),
    /// Intro/outro ranges and the following episode for the item that just started.
    Meta { item_id: String, segments: Vec<Segment>, prev: Option<Item>, next: Option<Item> },
    /// Episodes of one season for the player's episode panel (`seasons` is empty when unchanged).
    Episodes { series_id: String, seasons: Vec<Item>, season_id: String, episodes: Vec<Item> },
}

/// The library page the user opened from the home screen.
struct LibraryView {
    lib: Item,
    key: SortKey,
    page_size: usize,
    descending: bool,
    page: usize,
    items: Vec<Item>,
    total: usize,
    loading: bool,
    error: Option<String>,
}

/// Things the player overlay asked for during one frame, applied after drawing.
#[derive(Default)]
struct PlayerActions {
    back: bool,
    prev: bool,
    next: bool,
    toggle_panel: bool,
    pick_episode: Option<Item>,
    pick_season: Option<String>,
    toggle_season_menu: bool,
    remember_audio: Option<Track>,
    remember_sub: Option<Option<Track>>,
    open_series: Option<Item>,
}

pub struct App {
    cfg: Config,
    client: Option<Client>,
    tab: Tab,
    admin: bool,
    server_version: String,

    latest: Vec<Item>,
    resume: Vec<Item>,
    next_up: Vec<Item>,
    libraries: Vec<Item>,
    library: Option<LibraryView>,
    library_req: u64,
    detail: Vec<Detail>,
    detail_req: u64,
    search: SearchState,
    scroll_reset: bool,
    custom_scale_text: String,
    selected: Option<Item>,
    status: String,
    busy: bool,

    // login form
    password: String,
    form_url: String,
    form_user: String,
    new_name: String,
    view: View,
    manage: bool,

    tx: Sender<Msg>,
    rx: Receiver<Msg>,

    player: Option<Player>,
    player_error: Option<String>,
    controls_until: Instant,
    seek_drag: Option<f64>,
    last_progress: Instant,

    // per-playback extras
    segments: Vec<Segment>,
    next_ep: Option<Item>,
    prev_ep: Option<Item>,
    last_skip: Option<Skip>,
    audio_tracks: Vec<Track>,
    sub_tracks: Vec<Track>,
    tracks_at: Instant,
    nav_w: f32,
    scroll_seen: Instant,
    vol_hot: bool,
    tracks_applied: bool,
    play_started: Instant,
    window_title: String,

    // episode panel
    panel_open: bool,
    season_menu: bool,
    panel_seasons: Vec<Item>,
    panel_season: Option<String>,
    panel_episodes: Vec<Item>,

    discord_now: discord::Shared,
    discord_enabled: Arc<AtomicBool>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx);

        let cfg = Config::load();
        let (tx, rx) = channel();

        let (player, player_error) = match cc.get_proc_address {
            Some(loader) => {
                let ctx = cc.egui_ctx.clone();
                match Player::new(&cfg, loader, move || ctx.request_repaint()) {
                    Ok(p) => (Some(p), None),
                    Err(e) => (None, Some(format!("{e:#}"))),
                }
            }
            None => (None, Some("OpenGL loader unavailable".to_string())),
        };

        let discord_now: discord::Shared = Arc::new(Mutex::new(None));
        let discord_enabled = Arc::new(AtomicBool::new(cfg.discord_enabled));
        discord::spawn(discord_enabled.clone(), discord_now.clone());

        let (form_url, form_user) = (cfg.server_url.clone(), cfg.username.clone());
        let mut app = Self {
            client: (cfg.logged_in() && !(cfg.switcher_on_launch && !cfg.profiles.is_empty())).then(|| Client::from_config(&cfg)),
            cfg,
            tab: Tab::Home,
            admin: false,
            server_version: String::new(),
            latest: vec![],
            resume: vec![],
            next_up: vec![],
            libraries: vec![],
            library: None,
            library_req: 0,
            detail: vec![],
            detail_req: 0,
            search: SearchState::default(),
            scroll_reset: false,
            custom_scale_text: String::new(),
            selected: None,
            status: String::new(),
            busy: false,
            password: String::new(),
            form_url,
            form_user,
            new_name: String::new(),
            view: View::Normal,
            manage: false,
            tx,
            rx,
            player,
            player_error,
            controls_until: Instant::now(),
            seek_drag: None,
            last_progress: Instant::now(),
            segments: vec![],
            next_ep: None,
            prev_ep: None,
            last_skip: None,
            audio_tracks: vec![],
            sub_tracks: vec![],
            tracks_at: Instant::now(),
            nav_w: 92.0,
            scroll_seen: Instant::now() - Duration::from_secs(10),
            vol_hot: false,
            tracks_applied: false,
            play_started: Instant::now(),
            window_title: "callephiin".to_string(),
            panel_open: false,
            season_menu: false,
            panel_seasons: vec![],
            panel_season: None,
            panel_episodes: vec![],
            discord_now,
            discord_enabled,
        };
        if app.client.is_some() {
            app.refresh_home(&cc.egui_ctx);
        } else if !app.cfg.profiles.is_empty() {
            app.view = View::Switcher;
        }
        app
    }

    /// Scale for the home/settings content (the sidebar and player keep their own sizing).
    /// Automatic mode scales with the monitor's physical resolution relative to 1080p, divided
    /// by the OS DPI scale so it never stacks on top of Windows display scaling.
    fn content_scale(&self, ctx: &egui::Context) -> f32 {
        if self.cfg.ui_scale > 0.0 {
            return self.cfg.ui_scale.clamp(0.5, 3.0);
        }
        let ppp = ctx.pixels_per_point();
        match ctx.input(|i| i.viewport().monitor_size) {
            Some(m) => ((m.y * ppp / 1080.0) / ppp).clamp(1.0, 2.0),
            None => 1.0,
        }
    }

    // ---------------------------------------------------------------- background work

    fn refresh_home(&mut self, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else { return };
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        self.busy = true;
        std::thread::spawn(move || {
            let _ = tx.send(Self::home_msg(&client));
            ctx.request_repaint();
        });
    }

    fn home_msg(client: &Client) -> Msg {
        match (client.home_items(40), client.resume_items(12)) {
            (Ok(latest), resume) => Msg::Home {
                latest,
                resume: resume.unwrap_or_default(),
                next_up: client.next_up_items(16),
                libraries: client.views(),
                admin: client.is_admin(),
                version: client.server_version(),
            },
            (Err(e), _) => Msg::Error(format!("Could not load library: {e:#}")),
        }
    }

    /// Mark an item watched on the server, drop it from the rows right away, then reload them.
    fn mark_watched(&mut self, item: Item, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else { return };
        self.resume.retain(|i| i.id != item.id);
        self.next_up.retain(|i| i.id != item.id);
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = client.mark_played(&item.id);
            let _ = tx.send(Self::home_msg(&client));
            ctx.request_repaint();
        });
    }

    fn start_login(&mut self, ctx: &egui::Context) {
        let mut cfg = self.cfg.clone();
        cfg.server_url = self.form_url.trim().to_string();
        cfg.username = self.form_user.trim().to_string();
        let pw = self.password.clone();
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        self.busy = true;
        self.status = "Signing in…".into();
        std::thread::spawn(move || {
            let r = Client::authenticate(&cfg, &pw).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Login(r));
            ctx.request_repaint();
        });
    }

    fn start_play(&mut self, item: Item, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else { return };
        let (cfg, tx, ctx) = (self.cfg.clone(), self.tx.clone(), ctx.clone());
        self.status = format!("Loading {}…", item.display_title());
        std::thread::spawn(move || {
            let r = (|| -> anyhow::Result<PlayInfo> {
                let target = if item.kind == "Series" { client.episode_for_series(&item.id)? } else { item };
                client.play_info(&target, &cfg)
            })()
            .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Play(r));
            ctx.request_repaint();
        });
    }

    /// Report the current item as stopped, then start `item`.
    fn play_episode(&mut self, item: Item, ctx: &egui::Context) {
        if let (Some(p), Some(client)) = (self.player.as_ref(), self.client.clone()) {
            if let Some(info) = p.current.clone() {
                let pos = p.position();
                std::thread::spawn(move || client.report_stopped(&info, pos));
            }
        }
        self.start_play(item, ctx);
    }

    /// Load a season's episodes (and, when `with_seasons`, the season list) for the panel.
    fn load_panel(&self, ctx: &egui::Context, series_id: String, season_id: Option<String>, with_seasons: bool) {
        let Some(client) = self.client.clone() else { return };
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let seasons = if with_seasons || season_id.is_none() { client.seasons(&series_id) } else { vec![] };
            let Some(season_id) = season_id.or_else(|| seasons.first().map(|x| x.id.clone())) else { return };
            let episodes = client.season_episodes(&series_id, &season_id);
            let _ = tx.send(Msg::Episodes { series_id, seasons, season_id, episodes });
            ctx.request_repaint();
        });
    }

    fn current_item(&self) -> Option<Item> {
        self.player.as_ref().and_then(|p| p.current.as_ref()).map(|i| i.item.clone())
    }

    fn stop_playback(&mut self, ctx: &egui::Context) {
        if let (Some(p), Some(client)) = (self.player.as_mut(), self.client.clone()) {
            if let Some(info) = p.current.clone() {
                let pos = p.position();
                std::thread::spawn(move || client.report_stopped(&info, pos));
            }
            p.stop();
        }
        if let Ok(mut g) = self.discord_now.lock() {
            *g = None;
        }
        self.segments.clear();
        self.next_ep = None;
        self.prev_ep = None;
        self.last_skip = None;
        self.panel_open = false;
        self.season_menu = false;
        self.window_title = "callephiin".to_string();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title.clone()));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        self.refresh_home(ctx);
    }

    fn open_dashboard(&self) {
        let url = format!("{}/web/#/dashboard", self.cfg.server_url.trim_end_matches('/'));
        let _ = webbrowser::open(&url);
    }

    fn poll_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Login(Ok(cfg)) => {
                    self.reset_session();
                    self.cfg = cfg;
                    self.cfg.upsert_profile(&self.new_name);
                    self.cfg.save();
                    self.client = Some(Client::from_config(&self.cfg));
                    self.view = View::Normal;
                    self.manage = false;
                    self.new_name.clear();
                    self.password.clear();
                    self.status.clear();
                    self.refresh_home(ctx);
                }
                Msg::Login(Err(e)) => {
                    self.busy = false;
                    self.status = e;
                }
                Msg::Home { latest, resume, next_up, libraries, admin, version } => {
                    self.busy = false;
                    self.server_version = version;
                    self.admin = admin;
                    self.status.clear();
                    if self.selected.is_none() {
                        self.selected = resume.first().or(latest.first()).cloned();
                    }
                    self.latest = latest;
                    self.resume = resume;
                    self.next_up = next_up;
                    self.libraries = libraries;
                }
                Msg::Library { req, result } => {
                    if req == self.library_req {
                        if let Some(v) = self.library.as_mut() {
                            v.loading = false;
                            match result {
                                Ok((items, total)) => {
                                    v.items = items;
                                    v.total = total;
                                    v.error = None;
                                }
                                Err(e) => v.error = Some(e),
                            }
                        }
                    }
                }
                Msg::Title { req, result } => self.on_title_loaded(req, result),
                Msg::TitleSeason { req, season_id, episodes, extras } => self.on_title_season(req, season_id, episodes, extras),
                Msg::TitleRefresh { req, item, next, episodes } => self.on_title_refresh(req, item, next, episodes),
                Msg::Person { req, items } => self.on_person_loaded(req, items),
                Msg::Search { req, query, results } => self.on_search(req, query, results),
                Msg::Error(e) => {
                    self.busy = false;
                    if e.contains("401") {
                        // session expired: sign in again (profile name / server / user are kept)
                        let (url, user) = (self.cfg.server_url.clone(), self.cfg.username.clone());
                        self.cfg.sign_out();
                        self.cfg.save();
                        self.client = None;
                        self.reset_session();
                        self.open_sign_in(url, user);
                        self.status = "Session expired. Please sign in again.".into();
                        continue;
                    }
                    self.status = e;
                }
                Msg::Play(Ok(info)) => {
                    self.status.clear();
                    if let Some(p) = self.player.as_mut() {
                        p.apply_track_prefs(self.cfg.use_file_default_tracks, &self.cfg.preferred_sub_lang, &self.cfg.preferred_audio_lang);
                        if let Err(e) = p.load(info.clone()) {
                            self.status = format!("{e:#}");
                        } else if let Some(client) = self.client.clone() {
                            self.last_progress = Instant::now();
                            self.controls_until = Instant::now() + Duration::from_secs(3);
                            self.segments.clear();
                            self.next_ep = None;
                            self.prev_ep = None;
                            self.last_skip = None;
                            self.audio_tracks.clear();
                            self.sub_tracks.clear();
                            self.tracks_at = Instant::now() - Duration::from_secs(60);
                            self.tracks_applied = false;
                            self.play_started = Instant::now();

                            if self.panel_open {
                                if let Some(series) = info.item.series_id.clone() {
                                    self.load_panel(ctx, series, info.item.season_id.clone(), true);
                                }
                            }
                            let (tx, ctx2) = (self.tx.clone(), ctx.clone());
                            std::thread::spawn(move || {
                                client.report_start(&info);
                                let segments = client.segments(&info.item);
                                let (prev, next) = client.adjacent_episodes(&info.item);
                                let _ = tx.send(Msg::Meta { item_id: info.item.id.clone(), segments, prev, next });
                                ctx2.request_repaint();
                            });
                        }
                    } else {
                        self.status = "Player unavailable (libmpv failed to load)".into();
                    }
                }
                Msg::Play(Err(e)) => self.status = format!("Playback failed: {e}"),
                Msg::Meta { item_id, segments, prev, next } => {
                    if self.current_item().map(|i| i.id).as_deref() == Some(item_id.as_str()) {
                        self.segments = segments;
                        self.prev_ep = prev;
                        self.next_ep = next;
                    }
                }
                Msg::Episodes { series_id, seasons, season_id, episodes } => {
                    let same_series = self.current_item().and_then(|i| i.series_id).as_deref() == Some(series_id.as_str());
                    if same_series {
                        if !seasons.is_empty() {
                            self.panel_seasons = seasons;
                        }
                        self.panel_season = Some(season_id);
                        self.panel_episodes = episodes;
                    }
                }
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_messages(ctx);

        // Scrollbars are invisible unless the user is scrolling (wheel / touchpad) or dragging one.
        let scrolling = ctx.input(|i| i.raw_scroll_delta != egui::Vec2::ZERO || i.smooth_scroll_delta != egui::Vec2::ZERO);
        if scrolling {
            self.scroll_seen = Instant::now();
        }
        let recent = self.scroll_seen.elapsed() < Duration::from_millis(900);
        if recent {
            ctx.request_repaint_after(Duration::from_millis(950));
        }
        ctx.style_mut(|st| {
            let sc = &mut st.spacing.scroll;
            let shown = if recent { 0.55 } else { 0.0 };
            sc.dormant_background_opacity = 0.0;
            sc.dormant_handle_opacity = 0.0;
            sc.active_background_opacity = 0.0;
            sc.active_handle_opacity = shown;
            sc.interact_background_opacity = if recent { 0.25 } else { 0.0 };
            sc.interact_handle_opacity = if recent { 0.9 } else { 0.0 };
        });
        let wanted = crate::jellyfin::take_count_queue();
        if !wanted.is_empty() {
            if let Some(client) = self.client.clone() {
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    for (i, id) in wanted.iter().enumerate() {
                        crate::jellyfin::store_count(id, client.fetch_episode_count(id));
                        if i % 4 == 3 {
                            ctx.request_repaint();
                        }
                    }
                    ctx.request_repaint();
                });
            }
        }

        let playing = self.player.as_ref().map_or(false, |p| p.current.is_some());
        if playing {
            self.player_ui(ctx);
            return;
        }

        let s = self.content_scale(ctx);

        if self.view == View::Switcher {
            self.switcher_ui(ctx, s);
            return;
        }
        if !self.cfg.logged_in() || self.client.is_none() || self.view == View::Add {
            if self.view == View::Normal && !self.cfg.profiles.is_empty() {
                self.view = View::Switcher;
            } else {
                self.login_ui(ctx, s);
            }
            return;
        }

        // The sidebar shrinks to icons unless the pointer is over it.
        let nav_hot = ctx
            .input(|i| i.pointer.latest_pos())
            .map_or(false, |p| p.x <= self.nav_w + 4.0 && p.y >= 0.0);
        let nav_w = ctx.animate_value_with_time(egui::Id::new("nav_w"), if nav_hot { 92.0 } else { 58.0 }, 0.12);
        self.nav_w = nav_w;
        let compact = nav_w < 75.0;
        egui::SidePanel::left("nav")
            .exact_width(nav_w)
            .resizable(false)
            .frame(egui::Frame::none().fill(PANEL).inner_margin(egui::Margin::symmetric(10.0, 18.0)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui::logo(ui, if compact { 30.0 } else { 46.0 });
                    ui.add_space(if compact { 14.0 } else { 18.0 });
                    if ui::nav_button(ui, "home", "Home", self.tab == Tab::Home, 62.0, compact) {
                        self.go_tab(Tab::Home);
                    }
                    ui.add_space(6.0);
                    if ui::nav_button(ui, "search", "Search", self.tab == Tab::Search, 62.0, compact) {
                        self.go_tab(Tab::Search);
                    }
                    ui.add_space(6.0);
                    if ui::nav_button(ui, "⚙", "Settings", self.tab == Tab::Settings, 62.0, compact) {
                        self.go_tab(Tab::Settings);
                    }
                    if self.admin {
                        ui.add_space(6.0);
                        // Opens the Jellyfin admin dashboard in the default browser.
                        if ui::nav_button(ui, "🖥", "Admin", false, 62.0, compact) {
                            self.open_dashboard();
                        }
                    }
                });
                // profile avatar at the bottom of the sidebar
                ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                    let prof = self.cfg.profile(&self.cfg.last_profile).cloned();
                    if profiles::sidebar_avatar(ui, prof.as_ref(), if compact { 34.0 } else { 44.0 }).clicked() {
                        self.view = View::Switcher;
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin::symmetric(28.0 * s, 22.0 * s)))
            .show(ctx, |ui| {
                ui::scale_style(ui, s);
                match self.tab {
                    Tab::Home | Tab::Search if !self.detail.is_empty() => self.detail_ui(ui, ctx, s),
                    Tab::Search => self.search_ui(ui, ctx, s),
                    Tab::Home if self.library.is_some() => self.library_ui(ui, ctx, s),
                    Tab::Home => self.home_ui(ui, ctx, s),
                    Tab::Settings => self.settings_ui(ui, ctx, s),
                }
            });
    }
}

// ------------------------------------------------------------------------------ screens

impl App {

    fn home_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, s: f32) {
        let Some(client) = self.client.clone() else { return };
        let mut play: Option<Item> = None;
        let mut hover: Option<Item> = None;
        let mut open_lib: Option<Item> = None;
        let mut watched: Option<Item> = None;
        let mut open_title: Option<Item> = None;
        let mut open_item: Option<Item> = None;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if let Some(sel) = self.selected.clone() {
                let mut opts = ui::HeroOpts { link_title: true, ..Default::default() };
                let mut hero_sel = sel.clone();
                // spoiler control also covers the hero while a Next Up episode is highlighted
                let from_next_up = self.next_up.iter().any(|i| i.id == sel.id) && !self.resume.iter().any(|i| i.id == sel.id);
                if self.cfg.spoiler_control && from_next_up && sel.is_episode() && !sel.played() {
                    opts.blur_thumb = true;
                    hero_sel.overview = Some("(This episode's synopsis is currently hidden to prevent spoilers.)".into());
                }
                let act = ui::hero(ui, &client, &hero_sel, s, &opts);
                if act.open_title {
                    open_item = Some(if sel.is_episode() { sel.series_stub() } else { sel.clone() });
                }
                if act.play {
                    play = Some(sel);
                }
            }
            ui.add_space(18.0 * s);

            if !self.resume.is_empty() {
                ui.label(RichText::new("Continue watching").size(18.0 * s).strong());
                ui.add_space(8.0 * s);
                ui::resume_row(ui, &client, &self.resume, s, "resume_row", &mut play, &mut hover, &mut watched, &mut open_item, false);
                ui.add_space(18.0 * s);
            }
            if !self.next_up.is_empty() {
                ui.label(RichText::new("Next up").size(18.0 * s).strong());
                ui.add_space(8.0 * s);
                ui::resume_row(ui, &client, &self.next_up, s, "next_up_row", &mut play, &mut hover, &mut watched, &mut open_item, self.cfg.spoiler_control);
                ui.add_space(18.0 * s);
            }
            if !self.libraries.is_empty() {
                ui.label(RichText::new("Libraries").size(18.0 * s).strong());
                ui.add_space(8.0 * s);
                ui::library_row(ui, &client, &self.libraries, s, &mut open_lib);
                ui.add_space(18.0 * s);
            }
            ui.label(RichText::new("Recently added").size(18.0 * s).strong());
            ui.add_space(8.0 * s);
            ui::poster_row(ui, &client, &self.latest, s, "latest_row", &mut open_title, &mut hover);

            if self.busy {
                ui.add_space(12.0 * s);
                ui.spinner();
            }
            if !self.status.is_empty() {
                ui.add_space(10.0 * s);
                ui.label(RichText::new(&self.status).color(MUTED));
            }
        });

        if let Some(h) = hover {
            self.selected = Some(h);
        }
        if let Some(lib) = open_lib {
            self.open_library(lib, ctx);
        }
        if let Some(item) = open_item.take().or_else(|| open_title.take()) {
            self.open_title(item, ctx);
        }
        if let Some(item) = watched.take() {
            play = None;
            self.mark_watched(item, ctx);
        }
        if let Some(item) = play {
            self.start_play(item, ctx);
        }
    }

    fn open_library(&mut self, lib: Item, ctx: &egui::Context) {
        self.library = Some(LibraryView {
            lib,
            key: SortKey::Year,
            descending: true,
            page_size: PAGE_SIZES[0],
            page: 0,
            items: vec![],
            total: 0,
            loading: true,
            error: None,
        });
        self.scroll_reset = true;
        self.load_library(ctx);
    }

    /// (Re)fetch the current page of the open library.
    fn load_library(&mut self, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else { return };
        let Some(v) = self.library.as_mut() else { return };
        v.loading = true;
        self.library_req += 1;
        let (req, lib, key, desc, page, size) = (self.library_req, v.lib.clone(), v.key, v.descending, v.page, v.page_size);
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = client.library_page(&lib, key, desc, page, size).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Library { req, result });
            ctx.request_repaint();
        });
    }

    fn library_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, s: f32) {
        let Some(client) = self.client.clone() else { return };
        let Some(v) = self.library.as_ref() else { return };
        let (name, mut key, mut desc) = (v.lib.name.clone(), v.key, v.descending);
        let (page, total, loading, error) = (v.page, v.total, v.loading, v.error.clone());
        let items = v.items.clone();
        let mut size = v.page_size;
        let pages = ((total + size - 1) / size).max(1);

        let (mut back, mut sort_changed, mut new_page) = (false, false, None::<usize>);
        let mut size_changed = false;
        let mut play: Option<Item> = None;
        let mut open_item: Option<Item> = None;

        let mut area = egui::ScrollArea::vertical().id_salt("library_scroll").auto_shrink([false, false]);
        if std::mem::take(&mut self.scroll_reset) {
            area = area.vertical_scroll_offset(0.0);
        }
        area.show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("<  Back").clicked() {
                    back = true;
                }
                ui.label(RichText::new(&name).size(26.0 * s).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::ComboBox::from_id_salt("library_page_size")
                        .selected_text(format!("{size} per page"))
                        .show_ui(ui, |ui| {
                            for n in PAGE_SIZES {
                                if ui.selectable_label(n == size, format!("{n} per page")).clicked() {
                                    size = n;
                                    size_changed = true;
                                }
                            }
                        });
                    egui::ComboBox::from_id_salt("library_sort")
                        .selected_text(format!("Sort: {} · {}", key.name(), if desc { "Descending" } else { "Ascending" }))
                        .show_ui(ui, |ui| {
                            for k in SortKey::ALL {
                                for d in [false, true] {
                                    let label = format!("{} · {}", k.name(), if d { "Descending" } else { "Ascending" });
                                    if ui.selectable_label(k == key && d == desc, label).clicked() {
                                        key = k;
                                        desc = d;
                                        sort_changed = true;
                                    }
                                }
                            }
                        });
                });
            });
            ui.add_space(10.0 * s);
            page_controls(ui, page, pages, total, s, &mut new_page);
            ui.add_space(12.0 * s);

            if let Some(e) = &error {
                ui.colored_label(Color32::LIGHT_RED, format!("Could not load this library: {e}"));
            } else if items.is_empty() && !loading {
                ui.label(RichText::new("Nothing in this library yet.").color(MUTED));
            }
            ui::poster_grid(ui, &client, &items, s, &mut open_item);
            if loading {
                ui.add_space(8.0 * s);
                ui.spinner();
            }
            if !items.is_empty() {
                ui.add_space(12.0 * s);
                page_controls(ui, page, pages, total, s, &mut new_page);
            }
            ui.add_space(24.0 * s);
        });

        if back {
            self.library = None;
            return;
        }
        if sort_changed || size_changed {
            if let Some(v) = self.library.as_mut() {
                v.key = key;
                v.descending = desc;
                v.page_size = size;
                v.page = 0;
            }
            self.scroll_reset = true;
            self.load_library(ctx);
        } else if let Some(p) = new_page {
            if let Some(v) = self.library.as_mut() {
                v.page = p;
            }
            self.scroll_reset = true;
            self.load_library(ctx);
        }
        if let Some(item) = open_item {
            self.open_title(item, ctx);
        } else if let Some(item) = play {
            self.start_play(item, ctx);
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, s: f32) {
        let snapshot = |c: &Config| {
            (
                c.stream_mode,
                c.remote_bitrate_kbps,
                c.discord_enabled,
                c.sub_font_size,
                c.preferred_sub_lang.clone(),
                c.preferred_audio_lang.clone(),
                c.ui_scale,
                c.ui_scale_custom,
                c.use_file_default_tracks,
                c.remember_tracks,
                (c.external_tracks, c.skip_back_secs, c.skip_fwd_secs, c.spoiler_control, c.switcher_on_launch),
            )
        };
        let before = snapshot(&self.cfg);

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("Settings").size(26.0 * s).strong());
            ui.add_space(14.0 * s);

            ui::section(ui, "Playback", s, |ui| {
                // fixed-height row so every item is centred on the same line
                ui.allocate_ui_with_layout(vec2(ui.available_width(), 30.0 * s), Layout::left_to_right(Align::Center), |ui| {
                    ui.label("Skip back");
                    ui.add(egui::DragValue::new(&mut self.cfg.skip_back_secs).range(1..=600).suffix(" s"));
                    ui.add_space(16.0 * s);
                    ui.label("Skip forward");
                    ui.add(egui::DragValue::new(&mut self.cfg.skip_fwd_secs).range(1..=600).suffix(" s"));
                });
                ui.add_space(6.0 * s);
                ui.checkbox(&mut self.cfg.spoiler_control, "Spoiler Control (blur thumbnails and hide titles and synopses of unwatched episodes)");
                ui.add_space(6.0 * s);
                egui::ComboBox::from_label("Streaming")
                    .selected_text(match self.cfg.stream_mode {
                        StreamMode::Auto => "Auto (Direct Play on local network, transcode remotely)",
                        StreamMode::AlwaysDirect => "Always Direct Play",
                        StreamMode::AlwaysTranscode => "Always transcode",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.cfg.stream_mode, StreamMode::Auto, "Auto");
                        ui.selectable_value(&mut self.cfg.stream_mode, StreamMode::AlwaysDirect, "Always Direct Play");
                        ui.selectable_value(&mut self.cfg.stream_mode, StreamMode::AlwaysTranscode, "Always transcode");
                    });
                let mut mbps = self.cfg.remote_bitrate_kbps as f32 / 1000.0;
                ui.add(egui::Slider::new(&mut mbps, 1.0..=80.0).text("Remote bitrate cap (Mbps)"));
                self.cfg.remote_bitrate_kbps = (mbps * 1000.0) as u32;
                ui.label(
                    RichText::new(format!(
                        "Server detected as {}",
                        match self.client.as_ref().map(|c| c.is_local()) {
                            Some(true) => "local",
                            _ => "remote",
                        }
                    ))
                    .color(MUTED),
                );
            });

            ui::section(ui, "Subtitles & audio", s, |ui| {
                ui.add(egui::Slider::new(&mut self.cfg.sub_font_size, 24..=80).text("Subtitle size"));
                ui.checkbox(&mut self.cfg.use_file_default_tracks, "Use each file's default and forced tracks");
                if self.cfg.use_file_default_tracks {
                    ui.label(RichText::new("Audio and subtitles start with whatever the file marks as default or forced.").color(MUTED));
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Subtitle languages");
                        ui.text_edit_singleline(&mut self.cfg.preferred_sub_lang);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Audio languages");
                        ui.text_edit_singleline(&mut self.cfg.preferred_audio_lang);
                    });
                    ui.label(RichText::new("Comma-separated ISO codes in priority order, e.g. jpn,eng").color(MUTED));
                }
                ui.checkbox(&mut self.cfg.remember_tracks, "Remember my audio and subtitle choice for each show");
                ui.label(RichText::new("Styled ASS/SSA subtitles (e.g. anime) keep their own fonts and styling.").color(MUTED));
            });

            ui::section(ui, "Interface", s, |ui| {
                const PRESETS: [f32; 6] = [0.75, 1.0, 1.25, 1.5, 1.75, 2.0];
                let is_auto = self.cfg.ui_scale <= 0.0;
                let is_custom = !is_auto
                    && (self.cfg.ui_scale_custom || !PRESETS.iter().any(|p| (p - self.cfg.ui_scale).abs() < 0.001));
                let current = if is_auto {
                    "Automatic".to_string()
                } else if is_custom {
                    format!("Custom ({:.2}x)", self.cfg.ui_scale)
                } else {
                    format!("{:.2}x", self.cfg.ui_scale)
                };
                ui.horizontal(|ui| {
                    ui.label("Interface scale");
                    egui::ComboBox::from_id_salt("ui_scale_pick").selected_text(current).show_ui(ui, |ui| {
                        if ui.selectable_label(is_custom, "Custom").clicked() {
                            if self.cfg.ui_scale <= 0.0 {
                                self.cfg.ui_scale = (s * 4.0).round() / 4.0;
                            }
                            self.cfg.ui_scale_custom = true;
                            self.custom_scale_text = format!("{:.2}", self.cfg.ui_scale);
                        }
                        if ui.selectable_label(is_auto, "Automatic").clicked() {
                            self.cfg.ui_scale = 0.0;
                            self.cfg.ui_scale_custom = false;
                        }
                        for p in PRESETS {
                            let sel = !is_auto && !is_custom && (p - self.cfg.ui_scale).abs() < 0.001;
                            if ui.selectable_label(sel, format!("{p:.2}x")).clicked() {
                                self.cfg.ui_scale = p;
                                self.cfg.ui_scale_custom = false;
                            }
                        }
                    });
                });
                if is_custom {
                    ui.horizontal(|ui| {
                        ui.label("Custom scale (0.50x – 3.00x)");
                        if self.custom_scale_text.is_empty() {
                            self.custom_scale_text = format!("{:.2}", self.cfg.ui_scale);
                        }
                        let r = ui.add(egui::TextEdit::singleline(&mut self.custom_scale_text).desired_width(60.0 * s));
                        ui.label("x");
                        // applied on Enter / when the box loses focus, so the layout doesn't jump while typing
                        if r.lost_focus() {
                            match self.custom_scale_text.trim().trim_end_matches(['x', 'X']).parse::<f32>() {
                                Ok(v) if v.is_finite() => self.cfg.ui_scale = v.clamp(0.5, 3.0),
                                _ => {}
                            }
                            self.custom_scale_text = format!("{:.2}", self.cfg.ui_scale);
                        }
                    });
                    ui.label(RichText::new("Press Enter to apply.").color(MUTED));
                }
                ui.add_space(6.0 * s);
                ui.checkbox(&mut self.cfg.external_tracks, "Load external (sidecar) subtitle and audio files");
            });

            ui::section(ui, "Discord", s, |ui| {
                ui.checkbox(&mut self.cfg.discord_enabled, "Enable Discord Presence");
            });

            ui::section(ui, "Account", s, |ui| {
                let ver = if self.server_version.is_empty() { String::new() } else { format!(" | v{}", self.server_version) };
                ui.label(format!("{} @ {}{ver}", self.cfg.username, self.cfg.server_url));
                ui.horizontal(|ui| {
                    if ui.button("Switch profile").clicked() {
                        self.view = View::Switcher;
                    }
                    if ui.button("Sign out").clicked() {
                        self.sign_out_active();
                    }
                });
                ui.add_space(6.0 * s);
                ui.checkbox(&mut self.cfg.switcher_on_launch, "Always show the profile switcher at launch");
            });
            ui.add_space(18.0 * s);
            ui.hyperlink_to(format!("callephiin v{} | developed by callephi", env!("CARGO_PKG_VERSION")), "https://github.com/callephi/callephiin");
            ui.add_space(12.0 * s);
        });

        if before != snapshot(&self.cfg) {
            self.discord_enabled.store(self.cfg.discord_enabled, Ordering::Relaxed);
            self.cfg.save();
            ctx.request_repaint();
        }
    }

    // ------------------------------------------------------------------------ player

    fn apply_actions(&mut self, ctx: &egui::Context, a: PlayerActions) {
        if let Some(series) = a.open_series.clone() {
            self.stop_playback(ctx);
            if self.tab == Tab::Settings {
                self.tab = Tab::Home;
            }
            self.open_title(series, ctx);
            return;
        }
        // remember the user's own audio/subtitle choice for this show (matched by name later)
        if self.cfg.remember_tracks && (a.remember_audio.is_some() || a.remember_sub.is_some()) {
            if let Some(series) = self.current_item().filter(|i| i.is_episode()).and_then(|i| i.series_id) {
                let entry: &mut SeriesTracks = self.cfg.series_tracks.entry(series).or_default();
                if let Some(t) = &a.remember_audio {
                    entry.audio = Some(stored_track(t));
                }
                match &a.remember_sub {
                    Some(Some(t)) => {
                        entry.sub = Some(stored_track(t));
                        entry.sub_off = false;
                    }
                    Some(None) => entry.sub_off = true,
                    None => {}
                }
                self.cfg.save();
            }
        }
        if a.toggle_season_menu {
            self.season_menu = !self.season_menu;
        }
        if a.next {
            if let Some(n) = self.next_ep.clone() {
                return self.play_episode(n, ctx);
            }
        }
        if a.prev {
            if let Some(p) = self.prev_ep.clone() {
                return self.play_episode(p, ctx);
            }
        }
        if let Some(ep) = a.pick_episode {
            return self.play_episode(ep, ctx);
        }
        if let Some(season) = a.pick_season {
            self.season_menu = false;
            if let Some(series) = self.current_item().and_then(|i| i.series_id) {
                self.load_panel(ctx, series, Some(season), false);
            }
        }
        if a.toggle_panel {
            self.panel_open = !self.panel_open;
            self.season_menu = false;
            if self.panel_open {
                self.panel_episodes.clear();
                if let Some(item) = self.current_item() {
                    if let Some(series) = item.series_id.clone() {
                        self.load_panel(ctx, series, item.season_id.clone(), true);
                    }
                }
            }
        }
        if a.back {
            self.stop_playback(ctx);
        }
    }

    fn player_ui(&mut self, ctx: &egui::Context) {
        let Some(player) = self.player.as_ref() else { return };
        let Some(info) = player.current.clone() else { return };
        let client = self.client.clone();
        let mut act = PlayerActions::default();

        // window title follows what is playing: "callephiin - Show S1:E2 - Episode title"
        let wanted_title = {
            let it = &info.item;
            if it.is_episode() {
                match it.ep_code() {
                    Some(c) => format!("callephiin - {} {} - {}", it.display_title(), c, it.name),
                    None => format!("callephiin - {} - {}", it.display_title(), it.name),
                }
            } else {
                format!("callephiin - {}", it.display_title())
            }
        };
        if wanted_title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(wanted_title.clone()));
            self.window_title = wanted_title;
        }

        // --- sync Discord + server progress -----------------------------------------
        let pos = player.position();
        let dur = player.duration();
        let paused = player.paused();
        if let Ok(mut g) = self.discord_now.lock() {
            let item = &info.item;
            *g = Some(NowPlaying {
                title: item.display_title(),
                detail: if item.is_episode() { item.episode_label() } else { String::new() },
                position: pos,
                paused,
            });
        }
        if self.last_progress.elapsed() >= Duration::from_secs(10) {
            self.last_progress = Instant::now();
            if let Some(c) = client.clone() {
                let info = info.clone();
                std::thread::spawn(move || c.report_progress(&info, pos, paused));
            }
        }
        if player.ended() {
            // autoplay: roll on into the next episode (if any), otherwise leave the player
            if let Some(next) = self.next_ep.clone() {
                self.play_episode(next, ctx);
            } else {
                self.stop_playback(ctx);
            }
            return;
        }

        // --- track lists (refreshed about once a second, faster while a menu is open) ---------
        let popup_open = ctx.memory(|m| m.any_popup_open());
        // reading the track list costs several synchronous mpv calls per track, so only do it
        // while a menu is open, while waiting to apply remembered tracks, or after a pick
        let refresh_every = if popup_open {
            Duration::from_millis(400)
        } else if !self.tracks_applied {
            Duration::from_millis(600)
        } else {
            Duration::from_secs(1)
        };
        if self.tracks_at.elapsed() >= refresh_every {
            self.audio_tracks = player.tracks(TrackKind::Audio);
            self.sub_tracks = player.tracks(TrackKind::Sub);
            self.tracks_at = Instant::now();
        }
        if !self.tracks_applied && !self.audio_tracks.is_empty() && self.play_started.elapsed() > Duration::from_millis(1200) {
            self.tracks_applied = true;
            if self.cfg.remember_tracks {
                let series = info.item.series_id.clone().filter(|_| info.item.is_episode());
                if let Some(pref) = series.and_then(|s| self.cfg.series_tracks.get(&s).cloned()) {
                    if let Some(id) = pref.audio.as_ref().and_then(|w| find_match(&self.audio_tracks, w)) {
                        player.set_audio(id);
                    }
                    if pref.sub_off {
                        player.set_sub(None);
                    } else if let Some(id) = pref.sub.as_ref().and_then(|w| find_match(&self.sub_tracks, w)) {
                        player.set_sub(Some(id));
                    }
                    self.tracks_at = Instant::now() - Duration::from_secs(60);
                }
            }
        }

        // --- controls visibility -----------------------------------------------------
        let moved = ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_pressed());
        if moved || paused || popup_open || self.panel_open || self.seek_drag.is_some() || self.vol_hot {
            self.controls_until = Instant::now() + Duration::from_secs(3);
        }
        let show_controls = Instant::now() < self.controls_until;
        if !show_controls {
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }

        // --- keyboard ------------------------------------------------------------------
        let (space, left, right, esc, f_key) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Space),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::F),
            )
        });
        let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        if space { player.toggle_pause(); }
        if left { player.seek_by(-(self.cfg.skip_back_secs as f64)); }
        if right { player.seek_by(self.cfg.skip_fwd_secs as f64); }
        if f_key { ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen)); }
        if esc {
            if self.panel_open {
                act.toggle_panel = true;
            } else if fullscreen {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            } else {
                act.back = true;
            }
        }

        // --- video surface ----------------------------------------------------------------
        let screen = ctx.screen_rect();
        let render = player.render.clone();
        egui::CentralPanel::default().frame(egui::Frame::none().fill(Color32::BLACK)).show(ctx, |ui| {
            let cb = egui_glow::CallbackFn::new(move |info, _painter| {
                let [w, h] = info.screen_size_px;
                if let Ok(r) = render.lock() {
                    let _ = r.0.render::<()>(0, w as i32, h as i32, true);
                }
            });
            ui.painter().add(egui::PaintCallback { rect: screen, callback: Arc::new(cb) });
            let resp = ui.allocate_rect(screen, Sense::click());
            if resp.double_clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
            } else if resp.clicked() {
                player.toggle_pause();
            }
        });

        let panel_w = if self.panel_open { 440.0_f32.min(screen.width() * 0.45) } else { 0.0 };

        // --- Skip Intro / Play Next (fades in while inside a detected segment) -------------------
        let active = self
            .segments
            .iter()
            .find(|g| pos >= g.start - 0.25 && pos < g.end - 1.0)
            .and_then(|g| match g.kind {
                SegmentKind::Intro => Some(Skip::Intro(g.end)),
                SegmentKind::Outro if self.next_ep.is_some() => Some(Skip::Next),
                SegmentKind::Outro => None,
            });
        if active.is_some() {
            self.last_skip = active;
        }
        let fade = ctx.animate_bool_with_time(egui::Id::new("skip_fade"), active.is_some(), 0.4);
        if fade > 0.0 {
            if let Some(sk) = self.last_skip {
                let label = match sk {
                    Skip::Intro(_) => "Skip Intro",
                    Skip::Next => "Play Next",
                };
                let y = if show_controls { screen.height() - 210.0 } else { screen.height() - 112.0 };
                let alpha = (fade * 255.0) as u8;
                let mut clicked = false;
                egui::Area::new("skip_btn".into())
                    .fixed_pos(pos2(screen.right() - 48.0 - panel_w - 200.0, y))
                    .order(egui::Order::Foreground)
                    .interactable(active.is_some())
                    .show(ctx, |ui| {
                        let fill = Color32::from_rgba_unmultiplied(240, 240, 240, alpha);
                        let text = Color32::from_rgba_unmultiplied(0, 0, 0, alpha);
                        // text is painted in the exact centre of the button
                        clicked = ui::pill_button(ui, label, 18.0, 46.0, 24.0, 200.0, 8.0, fill, text).clicked();
                    });
                if clicked {
                    match sk {
                        Skip::Intro(end) => player.seek_to(end),
                        Skip::Next => act.next = true,
                    }
                }
            }
        }

        if !show_controls {
            ctx.request_repaint_after(Duration::from_millis(250));
            self.apply_actions(ctx, act);
            return;
        }

        // --- overlay: top (back + title logo) ------------------------------------------------
        let item = info.item.clone();
        let art_id = item.art_id().to_string();
        let has_logo = item.image_tags.contains_key("Logo") || item.parent_logo_image_tag.is_some();
        let logo_url = client.as_ref().map(|c| c.image_url(&art_id, "Logo", 260));

        egui::Area::new("player_top".into())
            .fixed_pos(pos2(0.0, 0.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui::vgradient(ui.painter(), Rect::from_min_size(pos2(0.0, 0.0), vec2(screen.width(), 200.0)),
                    Color32::from_black_alpha(190), Color32::TRANSPARENT);
                let r = Rect::from_min_size(pos2(24.0, 18.0), vec2(screen.width() - 48.0, 140.0));
                ui.allocate_new_ui(UiBuilder::new().max_rect(r), |ui| {
                    ui.horizontal(|ui| {
                        if ui::icon_button(ui, Icon::Back, 40.0, true).clicked() {
                            act.back = true;
                        }
                        ui.add_space(8.0);
                        if item.is_episode() {
                            ui.label(RichText::new(item.episode_label()).size(20.0).strong().color(Color32::WHITE));
                        }
                    });
                });
                // show / movie logo in the top-right corner
                let logo_rect = Rect::from_min_max(pos2(r.left(), r.top() + 18.0), pos2(r.right() - 36.0, r.bottom() + 20.0));
                ui.allocate_new_ui(UiBuilder::new().max_rect(logo_rect).layout(Layout::right_to_left(Align::TOP)), |ui| {
                    match (&logo_url, has_logo) {
                        (Some(url), true) => {
                            let r = ui.add(egui::Image::new(url.as_str()).max_height(120.0).max_width(520.0).show_loading_spinner(false).sense(Sense::click()));
                            if r.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if r.clicked() {
                                act.open_series = Some(if item.is_episode() { item.series_stub() } else { item.clone() });
                            }
                        }
                        _ => {
                            let r = ui.add(egui::Label::new(RichText::new(item.display_title()).size(30.0).strong().color(Color32::WHITE)).sense(Sense::click()));
                            if r.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if r.clicked() {
                                act.open_series = Some(if item.is_episode() { item.series_stub() } else { item.clone() });
                            }
                        }
                    }
                });
            });

        // --- overlay: episode panel (right side) ---------------------------------------------------
        if self.panel_open {
            let panel_rect = Rect::from_min_max(pos2(screen.right() - panel_w, 0.0), pos2(screen.right(), screen.height() - 150.0));
            let seasons = self.panel_seasons.clone();
            let multi_seasons = seasons.len() > 1;
            let spoil = ui::Spoiler { on: self.cfg.spoiler_control, next_id: self.next_ep.as_ref().map(|e| e.id.clone()) };
            let cur_season = self.panel_season.clone();
            let episodes = self.panel_episodes.clone();
            let cur_id = info.item.id.clone();
            let client_ref = client.clone();
            let season_menu = self.season_menu;
            egui::Area::new("episode_panel".into())
                .fixed_pos(panel_rect.min)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.painter().rect_filled(
                        panel_rect,
                        Rounding { nw: 16.0, sw: 16.0, ne: 0.0, se: 0.0 },
                        Color32::from_rgba_unmultiplied(10, 12, 18, 240),
                    );
                    // swallow clicks so they don't pause the video underneath
                    ui.interact(panel_rect, egui::Id::new("episode_panel_bg"), Sense::click());
                    let inner = panel_rect.shrink(16.0);
                    ui.allocate_new_ui(UiBuilder::new().max_rect(inner), |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Episodes").size(20.0).strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button("Close").clicked() {
                                    act.toggle_panel = true;
                                }
                                if multi_seasons {
                                    let name = seasons
                                        .iter()
                                        .find(|x| Some(&x.id) == cur_season.as_ref())
                                        .map(|x| x.name.clone())
                                        .unwrap_or_else(|| "Season".to_string());
                                    if ui::dropdown_button(ui, &ui::ellipsize(&name, 14), 15.0).clicked() {
                                        act.toggle_season_menu = true;
                                    }
                                }
                            });
                        });
                        ui.add_space(8.0);
                        if season_menu && multi_seasons {
                            // season list: five rows visible, scrolls beyond that
                            let row_h = 28.0_f32;
                            let gap = 2.0_f32;
                            egui::Frame::none()
                                .fill(Color32::from_rgba_unmultiplied(28, 31, 42, 255))
                                .rounding(10.0)
                                .inner_margin(6.0)
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = gap;
                                    egui::ScrollArea::vertical()
                                        .id_salt("season_scroll")
                                        .max_height(5.0 * row_h + 4.0 * gap)
                                        .auto_shrink([false, true])
                                        .show(ui, |ui| {
                                            for se in &seasons {
                                                let sel = Some(&se.id) == cur_season.as_ref();
                                                let w = ui.available_width();
                                                if ui.add_sized(vec2(w, row_h), egui::SelectableLabel::new(sel, &se.name)).clicked() {
                                                    act.pick_season = Some(se.id.clone());
                                                }
                                            }
                                        });
                                });
                            ui.add_space(8.0);
                        }
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            if episodes.is_empty() {
                                ui.label(RichText::new("Loading…").color(MUTED));
                            }
                            if let Some(c) = client_ref.as_ref() {
                                for ep in &episodes {
                                    let is_cur = ep.id == cur_id;
                                    if ui::episode_row(ui, c, ep, is_cur, inner.width() - 12.0, &spoil) && !is_cur {
                                        act.pick_episode = Some(ep.clone());
                                    }
                                }
                            }
                        });
                    });
                });
        }

        // --- overlay: bottom controls ---------------------------------------------------------------
        let (skip_back, skip_fwd) = (self.cfg.skip_back_secs as f64, self.cfg.skip_fwd_secs as f64);
        let mut volume = player.volume();
        let audio_tracks = self.audio_tracks.clone();
        let sub_tracks = self.sub_tracks.clone();
        let marks: Vec<SeekMark> = self
            .segments
            .iter()
            .map(|g| SeekMark {
                start: g.start,
                end: g.end,
                label: match g.kind {
                    SegmentKind::Intro => "Intro",
                    SegmentKind::Outro => "Credits",
                },
            })
            .collect();
        let (has_prev, has_next) = (self.prev_ep.is_some(), self.next_ep.is_some());
        let is_episode = info.item.is_episode();
        let mut refresh_tracks = false;
        egui::Area::new("player_bottom".into())
            .fixed_pos(pos2(0.0, screen.height() - 150.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui::vgradient(ui.painter(), Rect::from_min_size(pos2(0.0, screen.height() - 150.0), vec2(screen.width(), 150.0)),
                    Color32::TRANSPARENT, Color32::from_black_alpha(210));
                let r = Rect::from_min_size(pos2(28.0, screen.height() - 110.0), vec2(screen.width() - 56.0, 96.0));
                ui.allocate_new_ui(UiBuilder::new().max_rect(r), |ui| {
                    // time bar with intro / credits markers
                    let total = dur.max(1.0);
                    let shown = self.seek_drag.unwrap_or(pos);
                    ui.horizontal(|ui| {
                        ui.add_sized(vec2(64.0, 20.0), egui::Label::new(RichText::new(ui::fmt_time(shown)).monospace().color(Color32::WHITE)));
                        let bar_w = (ui.available_width() - 64.0 - ui.spacing().item_spacing.x).max(100.0);
                        match ui::seek_bar(ui, bar_w, shown, total, &marks) {
                            SeekEvent::Drag(t) => self.seek_drag = Some(t),
                            SeekEvent::Commit(t) => {
                                player.seek_to(t);
                                self.seek_drag = None;
                            }
                            SeekEvent::None => {}
                        }
                        ui.add_sized(vec2(64.0, 20.0), egui::Label::new(RichText::new(ui::fmt_time(total)).monospace().color(Color32::from_gray(190))));
                    });
                    ui.add_space(2.0);
                    ui.allocate_ui_with_layout(vec2(ui.available_width(), 44.0), Layout::left_to_right(Align::Center), |ui| {
                        if ui::icon_button(ui, Icon::Prev, 34.0, has_prev).clicked() && has_prev { act.prev = true; }
                        if ui::icon_button(ui, Icon::SkipBack, 34.0, true).on_hover_text(format!("Back {}s", skip_back)).clicked() { player.seek_by(-skip_back); }
                        let pp = if paused { Icon::Play } else { Icon::Pause };
                        if ui::icon_button(ui, pp, 40.0, true).clicked() {
                            player.toggle_pause();
                        }
                        if ui::icon_button(ui, Icon::SkipFwd, 34.0, true).on_hover_text(format!("Forward {}s", skip_fwd)).clicked() { player.seek_by(skip_fwd); }
                        if ui::icon_button(ui, Icon::Next, 34.0, has_next).clicked() && has_next { act.next = true; }
                        ui.add_space(18.0);

                        // Subtitle list
                        let sub_btn = ui::icon_button(ui, Icon::Subtitles, 36.0, true);
                        let sub_id = egui::Id::new("subtitle_menu");
                        if sub_btn.clicked() { ui.memory_mut(|m| m.toggle_popup(sub_id)); }
                        egui::popup_above_or_below_widget(ui, sub_id, &sub_btn, egui::AboveOrBelow::Above, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                            ui.set_min_width(280.0);
                            ui.label(RichText::new("Subtitles").strong());
                            ui.separator();
                            let none_selected = !sub_tracks.iter().any(|t| t.selected);
                            if ui.selectable_label(none_selected, "Off").clicked() {
                                player.set_sub(None);
                                act.remember_sub = Some(None);
                                refresh_tracks = true;
                            }
                            for t in &sub_tracks {
                                if ui.selectable_label(t.selected, ui::track_label(t)).clicked() {
                                    player.set_sub(Some(t.id));
                                    act.remember_sub = Some(Some(t.clone()));
                                    refresh_tracks = true;
                                }
                            }
                        });

                        // Audio list
                        let aud_btn = ui::icon_button(ui, Icon::Audio, 36.0, true);
                        let aud_id = egui::Id::new("audio_menu");
                        if aud_btn.clicked() { ui.memory_mut(|m| m.toggle_popup(aud_id)); }
                        egui::popup_above_or_below_widget(ui, aud_id, &aud_btn, egui::AboveOrBelow::Above, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                            ui.set_min_width(280.0);
                            ui.label(RichText::new("Audio").strong());
                            ui.separator();
                            if audio_tracks.is_empty() {
                                ui.label(RichText::new("No audio tracks").color(MUTED));
                            }
                            for t in &audio_tracks {
                                if ui.selectable_label(t.selected, ui::track_label(t)).clicked() {
                                    player.set_audio(t.id);
                                    act.remember_audio = Some(t.clone());
                                    refresh_tracks = true;
                                }
                            }
                        });

                        if info.transcoding {
                            ui.add_space(12.0);
                            ui.label(RichText::new("TRANSCODING").small().color(Color32::from_rgb(255, 190, 80)));
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            // rightmost: fullscreen; to its left: episode list
                            let fs_icon = if fullscreen { Icon::ExitFullscreen } else { Icon::Fullscreen };
                            if ui::icon_button(ui, fs_icon, 34.0, true).clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
                            }
                            if is_episode && ui::icon_button(ui, Icon::Episodes, 34.0, true).clicked() {
                                act.toggle_panel = true;
                            }
                            // volume: speaker icon; the slider (and percentage) appears while hovering it
                            let vol_icon = ui::icon_button(ui, Icon::Volume, 34.0, true);
                            let mut area = vol_icon.rect;
                            if self.vol_hot || vol_icon.hovered() {
                                ui.add_sized(vec2(44.0, 24.0), egui::Label::new(RichText::new(format!("{}%", volume.round() as i64)).color(Color32::from_gray(220))));
                                let (changed, sr) = ui::volume_slider(ui, &mut volume, 130.0, 120.0);
                                if changed {
                                    player.set_volume(volume);
                                }
                                area = area.union(sr.rect);
                            }
                            let near = area.expand2(vec2(10.0, 14.0));
                            self.vol_hot = ctx.input(|i| i.pointer.latest_pos()).map_or(false, |p| near.contains(p));
                        });
                    });
                });
            });

        if refresh_tracks {
            self.tracks_at = Instant::now() - Duration::from_secs(60);
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        self.apply_actions(ctx, act);
    }
}

/// egui's built-in fonts have no Japanese / Korean / Chinese glyphs, so borrow the system's CJK fonts.
fn install_cjk_fonts(ctx: &egui::Context) {
    const CANDIDATES: [&str; 14] = [
        r"C:\Windows\Fonts\YuGothM.ttc",
        r"C:\Windows\Fonts\YuGothR.ttc",
        r"C:\Windows\Fonts\meiryo.ttc",
        r"C:\Windows\Fonts\msgothic.ttc",
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\malgun.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
    ];
    let mut fonts = egui::FontDefinitions::default();
    let mut added = false;
    for (i, path) in CANDIDATES.iter().enumerate() {
        if let Ok(bytes) = std::fs::read(path) {
            let name = format!("cjk{i}");
            fonts.font_data.insert(name.clone(), egui::FontData::from_owned(bytes));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().push(name.clone());
            }
            added = true;
        }
    }
    if added {
        ctx.set_fonts(fonts);
    }
}

fn apply_theme(ctx: &egui::Context) {
    install_cjk_fonts(ctx);
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.override_text_color = Some(Color32::from_rgb(0xec, 0xee, 0xf4));
    v.selection.bg_fill = ACCENT;
    v.widgets.noninteractive.rounding = Rounding::same(10.0);
    v.widgets.inactive.rounding = Rounding::same(10.0);
    v.widgets.hovered.rounding = Rounding::same(10.0);
    v.widgets.active.rounding = Rounding::same(10.0);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x24, 0x28, 0x34);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x33, 0x38, 0x48);
    ctx.set_visuals(v);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = vec2(10.0, 8.0);
    style.spacing.button_padding = vec2(12.0, 6.0);
    ctx.set_style(style);
}

fn stored_track(t: &Track) -> StoredTrack {
    StoredTrack { lang: t.lang.clone(), title: t.title.clone(), codec: t.codec.clone(), forced: t.forced }
}

/// Find the track in `tracks` that best matches a remembered one, by names rather than index:
/// language + title (+ forced flag), then title alone, then language + codec, then language.
fn find_match(tracks: &[Track], w: &StoredTrack) -> Option<i64> {
    let eq = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    let lang_ok = !w.lang.is_empty();
    tracks
        .iter()
        .find(|t| eq(&t.lang, &w.lang) && eq(&t.title, &w.title) && t.forced == w.forced)
        .or_else(|| tracks.iter().find(|t| eq(&t.lang, &w.lang) && eq(&t.title, &w.title)))
        .or_else(|| if w.title.is_empty() { None } else { tracks.iter().find(|t| eq(&t.title, &w.title)) })
        .or_else(|| {
            if !lang_ok { return None; }
            tracks.iter().find(|t| eq(&t.lang, &w.lang) && t.forced == w.forced && eq(&t.codec, &w.codec))
        })
        .or_else(|| {
            if !lang_ok { return None; }
            tracks.iter().find(|t| eq(&t.lang, &w.lang) && t.forced == w.forced)
        })
        .map(|t| t.id)
}

/// "Previous  Page 2 of 7 · 168 titles  Next"
fn page_controls(ui: &mut egui::Ui, page: usize, pages: usize, total: usize, s: f32, new_page: &mut Option<usize>) {
    ui.horizontal(|ui| {
        if ui.add_enabled(page > 0, egui::Button::new("Previous")).clicked() {
            *new_page = Some(page - 1);
        }
        ui.label(RichText::new(format!("Page {} of {}  ·  {} titles", page + 1, pages, total)).size(14.0 * s).color(MUTED));
        if ui.add_enabled(page + 1 < pages, egui::Button::new("Next")).clicked() {
            *new_page = Some(page + 1);
        }
    });
}
