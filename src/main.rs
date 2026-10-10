#![cfg_attr(all(not(debug_assertions), not(test)), windows_subsystem = "windows")]

mod update;
mod transfer;
mod settings;
mod launcher_update;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod instance;

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, atomic::AtomicBool};
use std::thread;
use std::time::Duration;

use eframe::egui::{
    self, Align2, Button, Color32, CursorIcon, FontData, FontDefinitions, FontFamily, Id, Label, Rect,
    RichText, ScrollArea, Sense, Stroke, StrokeKind, TextEdit, TextureHandle, pos2, vec2,
};
use update::{CraftApp, Group, LogEntry, RemoteInfo, APPS};

const TEXT: Color32 = Color32::from_rgb(244, 242, 250);
const MUTED: Color32 = Color32::from_rgb(166, 162, 184);
const ACCENT: Color32 = Color32::from_rgb(124, 108, 242);
const UPDATE_TEXT: Color32 = Color32::from_rgb(190, 166, 255);
const BACKGROUND: Color32 = Color32::from_rgb(28, 25, 34);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Updates,
    Recent,
    Favorites,
    Group(Group),
}

enum Event {
    Status(String),
    Remote { id: String, info: RemoteInfo },
    ReloadLog(String),
    CheckDone(Vec<String>),
    Idle { status: Option<String> },
    Failed(String),
    Progress(transfer::Progress),
    LauncherChecked(Result<Option<launcher_update::Release>, String>),
    LauncherReady(std::path::PathBuf),
    Usage { id: String, size: Result<u64, String> },
}

#[derive(Clone)]
enum DownloadJob { App { id: String, repair: bool }, All, Launcher(launcher_update::Release) }

#[derive(Clone, Copy)]
enum UninstallTarget {
    App(usize),
    All,
}

struct Launcher {
    #[cfg(windows)]
    tray: Option<tray::SystemTray>,
    root: std::path::PathBuf,
    icons: HashMap<String, TextureHandle>,
    uninstalled_icons: HashMap<String, TextureHandle>,
    search: String,
    filter: Filter,
    selected: Option<usize>,
    installed: HashMap<String, String>,
    remote: HashMap<String, RemoteInfo>,
    logs: HashMap<String, Vec<LogEntry>>,
    recent: Vec<String>,
    favorites: Vec<String>,
    status: String,
    error: Option<String>,
    busy: Option<String>,
    checking: bool,
    settings: settings::Settings,
    settings_open: bool,
    cancel: Arc<AtomicBool>,
    progress: Option<transfer::Progress>,
    retry: Option<DownloadJob>,
    launcher_release: Option<launcher_update::Release>,
    launcher_checking: bool,
    launcher_status: String,
    launcher_ready: Option<std::path::PathBuf>,
    usage: HashMap<String, Result<u64, String>>,
    usage_selected: Option<usize>,
    delete_portable_data: bool,
    logo: TextureHandle,
    glass_hwnd: Option<isize>,
    glass_maximized: Option<bool>,
    window_shape: Option<(isize, i32, i32, i32)>,
    patch_app: Option<usize>,
    uninstall_target: Option<UninstallTarget>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
}

impl Launcher {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_style(&cc.egui_ctx);
        let root = update::artcraft_root();
        let recovery_error = update::recover_interrupted_installs(&root).err();
        let update_error_file = root.join("last-launcher-update-error.txt");
        let previous_update_error = std::fs::read_to_string(&update_error_file).ok();
        if previous_update_error.is_some() { let _ = std::fs::remove_file(&update_error_file); }
        update::ensure_baseline(&root);
        let (tx, rx) = mpsc::channel();
        let settings = settings::Settings::load(&root);
        if settings.check_on_startup { spawn_check(tx.clone()); }
        if settings.check_launcher_on_startup { spawn_launcher_check(tx.clone(), settings.beta_updates); }
        let checking = settings.check_on_startup;
        let launcher_checking = settings.check_launcher_on_startup;
        Self {
            #[cfg(windows)]
            tray: match tray::SystemTray::new(&cc.egui_ctx) {
                Ok(tray) => Some(tray),
                Err(error) => { eprintln!("Could not create system tray: {error}"); None }
            },
            icons: load_icons(&cc.egui_ctx, &root, false),
            uninstalled_icons: load_icons(&cc.egui_ctx, &root, true),
            installed: update::installed_versions(&root).into_iter().collect(),
            logs: APPS.iter().map(|app| (app.id.to_string(), update::load_log(&root, app.id))).collect(),
            recent: update::load_recent(&root),
            favorites: update::load_favorites(&root),
            root,
            search: String::new(),
            filter: Filter::All,
            selected: None,
            remote: HashMap::new(),
            status: if checking { "Checking for updates...".into() } else { "Ready".into() },
            error: recovery_error.or(previous_update_error),
            busy: None,
            checking,
            settings,
            settings_open: false,
            cancel: Arc::new(AtomicBool::new(false)),
            progress: None,
            retry: None,
            launcher_release: None,
            launcher_checking,
            launcher_status: String::new(),
            launcher_ready: None,
            usage: HashMap::new(),
            usage_selected: None,
            delete_portable_data: false,
            logo: load_logo(&cc.egui_ctx),
            glass_hwnd: None,
            glass_maximized: None,
            window_shape: None,
            patch_app: None,
            uninstall_target: None,
            tx,
            rx,
        }
    }

    fn poll(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Status(text) => self.status = text,
                Event::Remote { id, info } => {
                    self.remote.insert(id, info);
                }
                Event::ReloadLog(id) => {
                    self.logs.insert(id.clone(), update::load_log(&self.root, &id));
                    self.installed = update::installed_versions(&self.root).into_iter().collect();
                    self.recent = update::load_recent(&self.root);
                }
                Event::CheckDone(errors) => {
                    self.checking = false;
                    if !errors.is_empty() {
                        self.report_error(format!("Couldn't check updates for {}. Check your connection and try Check for updates again.", errors.join(", ")));
                    } else if self.busy.is_none() {
                        self.status = self.ready_status();
                    }
                }
                Event::Failed(error) => if error == transfer::CANCELLED { self.status = "Cancelled. Existing apps were kept.".into(); } else { self.report_error(error); },
                Event::Progress(progress) => self.progress = Some(progress),
                Event::LauncherChecked(result) => {
                    self.launcher_checking = false;
                    match result {
                        Ok(release) => {
                            self.launcher_status = if release.is_some() { "A new launcher version is available".into() } else { "Launcher is up to date".into() };
                            self.launcher_release = release;
                        }
                        Err(error) => { self.launcher_status = format!("Couldn't check launcher updates: {error}"); }
                    }
                }
                Event::LauncherReady(path) => { self.launcher_ready = Some(path); self.settings_open = true; }
                Event::Usage { id, size } => { self.usage.insert(id, size); }
                Event::Idle { status } => {
                    self.busy = None;
                    self.progress = None;
                    self.usage.clear();
                    self.usage_selected = None;
                    self.installed = update::installed_versions(&self.root).into_iter().collect();
                    if let Some(status) = status {
                        self.retry = None;
                        self.status = status;
                    }
                }
            }
        }
    }

    fn ready_status(&self) -> String {
        let count = self.update_count();
        if count == 0 {
            "Everything is up to date".into()
        } else {
            format!("{count} updates available")
        }
    }

    fn update_count(&self) -> usize {
        APPS.iter().filter(|app| self.has_update(app.id)).count()
    }

    fn has_update(&self, id: &str) -> bool {
        let Some(remote) = self.remote.get(id) else { return false };
        self.installed.get(id).map(String::as_str).unwrap_or("") != remote.tag
    }

    fn visible(&self) -> Vec<usize> {
        let query = self.search.trim().to_lowercase();
        let matches = |app: &CraftApp| {
            query.is_empty() || app.name.to_lowercase().contains(&query) || app.id.contains(&query)
        };
        if self.filter == Filter::Recent {
            return self
                .recent
                .iter()
                .filter_map(|id| APPS.iter().position(|app| app.id == id))
                .filter(|index| matches(&APPS[*index]))
                .collect();
        }
        APPS.iter()
            .enumerate()
            .filter(|(_, app)| matches(app) && self.filter_matches(app))
            .map(|(index, _)| index)
            .collect()
    }

    fn filter_matches(&self, app: &CraftApp) -> bool {
        match self.filter {
            Filter::All => true,
            Filter::Updates => self.has_update(app.id),
            Filter::Recent => false,
            Filter::Favorites => self.favorites.iter().any(|id| id == app.id),
            Filter::Group(group) => app.group == group,
        }
    }

    fn nav_count(&self, filter: Filter) -> usize {
        match filter {
            Filter::All => APPS.len(),
            Filter::Updates => self.update_count(),
            Filter::Recent => self.recent.len(),
            Filter::Favorites => self.favorites.len(),
            Filter::Group(group) => APPS.iter().filter(|app| app.group == group).count(),
        }
    }

    fn toggle_favorite(&mut self, id: &str) {
        let mut favorites = self.favorites.clone();
        if let Some(index) = favorites.iter().position(|favorite| favorite == id) {
            favorites.remove(index);
        } else {
            favorites.push(id.to_string());
        }
        match update::save_favorites(&self.root, &favorites) {
            Ok(()) => self.favorites = favorites,
            Err(error) => self.report_error(error),
        }
    }

    fn start_one(&mut self, id: &str) {
        self.start_download(DownloadJob::App { id: id.to_string(), repair: false });
    }

    fn start_check(&mut self) {
        if self.busy.is_some() || self.checking { return; }
        self.checking = true;
        self.status = "Checking for updates...".into();
        spawn_check(self.tx.clone());
    }

    fn start_all(&mut self) { self.start_download(DownloadJob::All); }

    fn start_download(&mut self, job: DownloadJob) {
        if self.busy.is_some() { return; }
        self.busy = Some(match &job {
            DownloadJob::App { id, .. } => id.clone(),
            DownloadJob::All => "*".into(),
            DownloadJob::Launcher(_) => "launcher".into(),
        });
        self.error = None;
        self.retry = Some(job.clone());
        self.cancel = Arc::new(AtomicBool::new(false));
        self.progress = None;
        self.status = "Preparing download...".into();
        let root = self.root.clone();
        let cancel = self.cancel.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let mut status = |message| { let _ = tx.send(Event::Status(message)); };
            let mut progress = |value| { let _ = tx.send(Event::Progress(value)); };
            let result = match job {
                DownloadJob::App { id, repair } => update::update_app(&root, &id, repair, &cancel, &mut status, &mut progress),
                DownloadJob::All => {
                    let mut errors = Vec::new();
                    for (index, app) in APPS.iter().enumerate() {
                        if transfer::check(&cancel).is_err() { errors.clear(); errors.push(transfer::CANCELLED.into()); break; }
                        status(format!("App {} of {}: {}", index + 1, APPS.len(), app.name));
                        if let Err(error) = update::update_app(&root, app.id, false, &cancel, &mut status, &mut progress) {
                            if error == transfer::CANCELLED { errors.clear(); errors.push(error); break; }
                            errors.push(error);
                        }
                        let _ = tx.send(Event::ReloadLog(app.id.into()));
                    }
                    if errors.is_empty() { Ok(update::UpdateOutcome { status: "All apps updated".into() }) }
                    else { Err(errors.join("\n")) }
                }
                DownloadJob::Launcher(release) => launcher_update::prepare(&root, &release, &cancel, &mut progress).map(|path| {
                    let _ = tx.send(Event::LauncherReady(path));
                    update::UpdateOutcome { status: "Launcher update ready. Open Settings to restart and update.".into() }
                }),
            };
            for app in APPS { let _ = tx.send(Event::ReloadLog(app.id.into())); }
            finish(&tx, result);
        });
    }
    fn report_error(&mut self, error: String) {
        self.status = error.clone();
        self.error = Some(error);
    }

    fn open_index(&mut self, index: usize) -> bool {
        let id = APPS[index].id;
        let name = APPS[index].name;
        match update::open_app(&self.root, id) {
            Ok(()) => {
                self.recent = update::remember_recent(&self.root, id);
                self.status = format!("Opened {name}");
                true
            }
            Err(error) => { self.report_error(error); false }
        }
    }

    fn start_uninstall(&mut self, target: UninstallTarget) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some("uninstall".into());
        self.retry = None;
        self.progress = None;
        self.status = "Uninstalling...".into();
        let root = self.root.clone();
        let tx = self.tx.clone();
        let delete_data = self.delete_portable_data;
        thread::spawn(move || {
            let apps: Vec<&CraftApp> = match target {
                UninstallTarget::App(index) => vec![&APPS[index]],
                UninstallTarget::All => APPS.iter().filter(|app| update::is_installed(&root, app.id)).collect(),
            };
            let mut errors = Vec::new();
            let mut removed = 0;
            for app in apps {
                let _ = tx.send(Event::Status(format!("Uninstalling {}...", app.name)));
                match update::uninstall_app_with_data(&root, app.id, delete_data) {
                    Ok(_) => removed += 1,
                    Err(error) => errors.push(format!("{}: {error}", app.name)),
                }
                let _ = tx.send(Event::ReloadLog(app.id.to_string()));
            }
            let status = if errors.is_empty() {
                match target {
                    UninstallTarget::App(index) => format!("{} uninstalled", APPS[index].name),
                    UninstallTarget::All => format!("Uninstalled {removed} apps"),
                }
            } else {
                let _ = tx.send(Event::Failed(errors.join("\n")));
                format!("Uninstalled {removed} apps · {}", errors.join(" · "))
            };
            let _ = tx.send(Event::Idle { status: Some(status) });
        });
    }

    fn uninstall_confirmation(&mut self, ctx: &egui::Context) {
        let Some(target) = self.uninstall_target else { return };
        let mut confirm = false;
        let mut cancel = false;
        let response = egui::Modal::new(Id::new("confirm-uninstall"))
            .frame(egui::Frame::popup(&ctx.style()).inner_margin(24.0_f32).corner_radius(16.0_f32))
            .show(ctx, |ui| {
            ui.set_width(380.0_f32);
            ui.style_mut().text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
            ui.style_mut().text_styles.insert(egui::TextStyle::Heading, egui::FontId::proportional(20.0));
            let title = match target {
                UninstallTarget::App(index) => format!("Uninstall {}?", APPS[index].name),
                UninstallTarget::All => "Uninstall all apps?".into(),
            };
            ui.heading(title);
            ui.add_space(12.0_f32);
            ui.label("Remove the app files. Portable settings and projects are kept unless you choose to delete them below.");
            ui.scope(|ui| {
                ui.spacing_mut().icon_width = 20.0;
                ui.spacing_mut().icon_width_inner = 14.0;
                ui.spacing_mut().interact_size.y = 32.0;
                ui.add(egui::Checkbox::new(&mut self.delete_portable_data,
                    RichText::new("Delete portable settings and projects too").size(15.0)));
            });
            if self.delete_portable_data { ui.label(RichText::new("Deleting portable data cannot be undone.").color(Color32::from_rgb(255, 170, 170))); }
            ui.add_space(8.0_f32);
            ui.label("Files saved elsewhere and your update history will be kept. Close the apps before uninstalling.");
            ui.add_space(16.0_f32);
            ui.allocate_ui_with_layout(vec2(ui.available_width(), 34.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                confirm = ui.add_enabled(self.busy.is_none(), uninstall_button_with_font("Uninstall", 15.0)).clicked();
                cancel = ui.add(ghost_button_with_font("Cancel", 15.0)).clicked();
            });
        });
        if confirm {
            self.uninstall_target = None;
            self.start_uninstall(target);
        } else if cancel || response.should_close() {
            self.uninstall_target = None;
        }
    }
}

fn finish(tx: &Sender<Event>, result: update::Result<update::UpdateOutcome>) {
    match result {
        Ok(outcome) => {
            let _ = tx.send(Event::Idle { status: Some(outcome.status) });
        }
        Err(error) => {
            let _ = tx.send(Event::Failed(error));
            let _ = tx.send(Event::Idle { status: None });
        }
    }
}

fn spawn_check(tx: Sender<Event>) {
    thread::spawn(move || {
        let mut handles = Vec::new();
        for app in APPS {
            let tx = tx.clone();
            handles.push(thread::spawn(move || {
                match update::fetch_remote(app.id) {
                    Ok(info) => {
                        let _ = tx.send(Event::Remote { id: app.id.to_string(), info });
                        None
                    }
                    Err(_) => Some(app.name.to_string()),
                }
            }));
        }
        let errors = handles.into_iter().filter_map(|handle| {
            handle.join().unwrap_or_else(|_| Some("an app".into()))
        }).collect();
        let _ = tx.send(Event::CheckDone(errors));
    });
}

fn spawn_launcher_check(tx: Sender<Event>, include_beta: bool) {
    thread::spawn(move || { let _ = tx.send(Event::LauncherChecked(launcher_update::check(include_beta))); });
}

fn main() -> eframe::Result<()> {
    #[cfg(windows)]
    let _instance = match instance::SingleInstance::acquire()
        .map_err(|error| eframe::Error::AppCreation(Box::new(error)))? {
        Some(instance) => instance,
        None => { instance::restore_existing(); return Ok(()); }
    };
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1220.0, 780.0])
        .with_min_inner_size([980.0, 620.0])
        .with_decorations(false)
        .with_transparent(true)
        .with_title("CraftLauncher");
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    eframe::run_native(
        "CraftLauncher",
        eframe::NativeOptions { viewport, ..Default::default() },
        Box::new(|cc| Ok(Box::new(Launcher::new(cc)))),
    )
}

impl eframe::App for Launcher {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        BACKGROUND.to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(windows)]
        if ctx.input(|i| i.viewport().close_requested()) {
            if let Some(tray) = &self.tray {
                if !tray.exiting() && self.settings.close_to_tray {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    tray.hide();
                }
            }
        }
        self.poll();
        if let Some(index) = self.selected {
            if self.usage_selected != Some(index) {
                self.usage_selected = Some(index);
                let id = APPS[index].id.to_string();
                let root = self.root.clone();
                let tx = self.tx.clone();
                thread::spawn(move || { let _ = tx.send(Event::Usage { size: update::app_disk_usage(&root, &id), id }); });
            }
        }
        self.apply_glass(ctx);
        if self.busy.is_some() || self.checking || self.launcher_checking || self.selected.is_some() {
            ctx.request_repaint_after(Duration::from_millis(80));
        }

        let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let frame = egui::Frame::NONE.fill(Color32::TRANSPARENT);
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            let full = ui.max_rect();
            let places = layout(full);
            paint_card(ui, places.card);

            // Sidebar header (logo & brand)
            self.sidebar_header(ui, places.sidebar_header, is_maximized);

            // Top bar (search & window control buttons)
            self.top_bar(ui, &places, is_maximized);

            // Sidebar (categories & update-all)
            self.sidebar(ui, places.side, places.nav, places.update_all);

            // Divider line between sidebar and main area
            let divider_x = places.side.max.x + 14.0_f32;
            ui.painter().line_segment(
                [
                    pos2(divider_x, places.inner.min.y + 4.0_f32),
                    pos2(divider_x, places.inner.max.y - 4.0_f32),
                ],
                Stroke::new(1.0_f32, Color32::from_white_alpha(16)),
            );

            // 5-column app grid
            self.grid(ui, places.grid);

            // Bottom status line
            self.status_line(ui, places.status);
            if self.patch_app.is_some() {
                egui::Area::new(Id::new("update-log-overlay"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(full.min)
                    .movable(false)
                    .show(ctx, |ui| {
                        ui.set_min_size(full.size());
                        self.patch_popup(ui, full);
                    });
            }
        });
        if self.selected.is_some() && self.patch_app.is_none() {
            let response = egui::Modal::new(Id::new("app-details-popup"))
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| self.app_details(ui));
            if response.inner {
                if let Some(index) = self.selected {
                    self.start_one(APPS[index].id);
                }
            }
            if self.uninstall_target.is_none() && self.patch_app.is_none() && response.should_close() {
                self.selected = None;
            }
        }
        self.download_panel(ctx);
        self.settings_panel(ctx);
        self.uninstall_confirmation(ctx);
        if let Some(error) = self.error.clone() {
            let response = egui::Modal::new(Id::new("operation-error"))
                .frame(egui::Frame::popup(&ctx.style()).inner_margin(24.0).corner_radius(16.0))
                .show(ctx, |ui| {
                    ui.set_width(460.0);
                    ui.label(RichText::new("Something went wrong").size(20.0).color(TEXT));
                    ui.add_space(12.0);
                    ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                        ui.label(RichText::new(&error).color(TEXT));
                    });
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        let close = ui.add(ghost_button("Close")).clicked();
                        let retry = self.retry.is_some() && self.busy.is_none() && ui.add(ghost_button("Retry")).clicked();
                        (close, retry)
                    }).inner
                });
            if response.inner.0 || response.inner.1 || response.should_close() { self.error = None; }
            if response.inner.1 { if let Some(job) = self.retry.clone() { self.start_download(job); } }
        }
    }
}

struct Places {
    card: Rect,
    inner: Rect,
    sidebar_header: Rect,
    minimize: Rect,
    maximize: Rect,
    close: Rect,
    side: Rect,
    nav: Rect,
    update_all: Rect,
    search: Rect,
    top_drag: Rect,
    grid: Rect,
    status: Rect,
}

fn layout(full: Rect) -> Places {
    let card = full;
    let inner = Rect::from_min_max(card.min + vec2(20.0_f32, 16.0_f32), card.max - vec2(20.0_f32, 16.0_f32));

    // Sidebar: width 224px
    let side_w = 224.0_f32;
    let side = Rect::from_min_max(inner.min, pos2(inner.min.x + side_w, inner.max.y));
    let sidebar_header = Rect::from_min_max(
        side.min,
        pos2(side.max.x, side.min.y + 42.0_f32),
    );
    let update_all = Rect::from_min_max(pos2(side.min.x, side.max.y - 42.0_f32), side.max);
    let nav = Rect::from_min_max(
        pos2(side.min.x, sidebar_header.max.y + 12.0_f32),
        pos2(side.max.x, update_all.min.y - 56.0_f32),
    );

    // Main area starts after divider
    let main_min_x = side.max.x + 28.0_f32;
    let main = Rect::from_min_max(pos2(main_min_x, inner.min.y), inner.max);

    // Top Bar in Main area (aligned with sidebar header)
    let top_bar = Rect::from_min_size(main.min, vec2(main.width(), 42.0_f32));
    let header_cy = top_bar.center().y;

    // Window controls on the top-right
    let close = Rect::from_center_size(pos2(top_bar.right() - 14.0_f32, header_cy), vec2(28.0_f32, 28.0_f32));
    let maximize = Rect::from_center_size(pos2(close.left() - 18.0_f32, header_cy), vec2(28.0_f32, 28.0_f32));
    let minimize = Rect::from_center_size(pos2(maximize.left() - 18.0_f32, header_cy), vec2(28.0_f32, 28.0_f32));

    // Search bar on top-left of main area with clean, balanced width
    let search_w = 320.0_f32.min((minimize.left() - main.min.x - 60.0_f32).max(180.0_f32));
    let search = Rect::from_min_max(
        pos2(main.min.x, top_bar.top() + 2.0_f32),
        pos2(main.min.x + search_w, top_bar.top() + 40.0_f32),
    );

    // Draggable space across top bar extending to top window edge
    let top_drag = Rect::from_min_max(
        pos2(search.right() + 8.0_f32, card.min.y),
        pos2(minimize.left() - 8.0_f32, top_bar.max.y),
    );

    // Bottom status bar
    let status = Rect::from_min_max(pos2(main.min.x, main.max.y - 24.0_f32), main.max);

    // The app grid keeps its full width when details are open.
    let work = Rect::from_min_max(
        pos2(main.min.x, top_bar.max.y + 16.0_f32),
        pos2(main.max.x, status.min.y - 8.0_f32),
    );
    let grid = work;

    Places {
        card,
        inner,
        sidebar_header,
        minimize,
        maximize,
        close,
        side,
        nav,
        update_all,
        search,
        top_drag,
        grid,
        status,
    }
}

fn paint_card(ui: &mut egui::Ui, card: Rect) {
    // Use the same opaque color as the framebuffer, including edge pixels.
    // The native window region owns the outer curve.
    ui.painter().rect_filled(card, 0.0_f32, BACKGROUND);
}

impl Launcher {
    fn apply_glass(&mut self, ctx: &egui::Context) {
        let title: Vec<u16> = "CraftLauncher".encode_utf16().chain(std::iter::once(0)).collect();
        let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
        if hwnd == 0 {
            return;
        }

        let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));

        // Keep native framing free of a second translucent background.
        self.clip_window_corners(hwnd, ctx.pixels_per_point(), is_maximized);
        if self.glass_hwnd != Some(hwnd) {
            unsafe {
                let backdrop: u32 = 1; // DWMSBT_NONE
                DwmSetWindowAttribute(hwnd, 38, &backdrop as *const u32 as _, 4);
                let dark: u32 = 1;
                DwmSetWindowAttribute(hwnd, 20, &dark as *const u32 as _, 4);
                let border_none: u32 = 0xFFFFFFFE;
                DwmSetWindowAttribute(hwnd, 34, &border_none as *const u32 as _, 4);

                let corners: u32 = if is_maximized { 1 } else { 2 };
                DwmSetWindowAttribute(hwnd, 33, &corners as *const u32 as _, 4);

                let policy = AccentPolicy { state: 0, flags: 0, gradient: 0, animation_id: 0 };
                let mut data = WindowCompositionAttributeData {
                    attribute: 19,
                    data: &policy as *const AccentPolicy as *mut _,
                    size: std::mem::size_of::<AccentPolicy>(),
                };
                let user32: Vec<u16> = "user32.dll\0".encode_utf16().collect();
                let module = GetModuleHandleW(user32.as_ptr());
                let proc = GetProcAddress(module, b"SetWindowCompositionAttribute\0".as_ptr());
                if !proc.is_null() {
                    let set: unsafe extern "system" fn(isize, *mut WindowCompositionAttributeData) -> i32 =
                        std::mem::transmute(proc);
                    set(hwnd, &mut data);
                }
            }
            self.glass_hwnd = Some(hwnd);
            self.glass_maximized = Some(is_maximized);
            return;
        }

        // Only update corner rounding attribute when maximize state actually changes (avoids composition reset and flicker)
        if self.glass_maximized != Some(is_maximized) {
            unsafe {
                let corners: u32 = if is_maximized { 1 } else { 2 };
                DwmSetWindowAttribute(hwnd, 33, &corners as *const u32 as _, 4);
            }
            self.glass_maximized = Some(is_maximized);
        }
    }

    fn clip_window_corners(&mut self, hwnd: isize, scale: f32, maximized: bool) {
        if unsafe { IsIconic(hwnd) } != 0 { return; }
        let mut bounds = NativeRect { left: 0, top: 0, right: 0, bottom: 0 };
        if unsafe { GetWindowRect(hwnd, &mut bounds) } == 0 { return; }
        let width = bounds.right - bounds.left;
        let height = bounds.bottom - bounds.top;
        let diameter = if maximized { 0 } else { (40.0 * scale).round() as i32 };
        let shape = (hwnd, width, height, diameter);
        if self.window_shape == Some(shape) || width <= 0 || height <= 0 { return; }
        unsafe {
            let region = if maximized { 0 } else {
                CreateRoundRectRgn(0, 0, width, height, diameter, diameter)
            };
            if !maximized && region == 0 { return; }
            // Windows owns the region after success; release it only on failure.
            if SetWindowRgn(hwnd, region, 1) != 0 {
                self.window_shape = Some(shape);
            } else if region != 0 {
                DeleteObject(region);
            }
        }
    }

    fn download_panel(&mut self, ctx: &egui::Context) {
        if self.retry.is_none() || (self.selected.is_some() && self.patch_app.is_none()) { return; }
        egui::Window::new("Downloads").anchor(Align2::CENTER_BOTTOM, vec2(0.0, -44.0))
            .resizable(false).collapsible(false).default_width(420.0).show(ctx, |ui| self.download_controls(ui));
    }

    fn download_controls(&mut self, ui: &mut egui::Ui) {
        let mut retry = false;
        ui.label(&self.status);
        let size_text = self.progress.as_ref().map(|p| {
            if let Some(total) = p.total.filter(|v| *v > 0) {
                ui.add(egui::ProgressBar::new((p.received as f64 / total as f64) as f32).show_percentage());
                format!("{} / {}", transfer::size(p.received), transfer::size(total))
            } else {
                ui.spinner();
                format!("{} downloaded", transfer::size(p.received))
            }
        });
        ui.allocate_ui_with_layout(vec2(ui.available_width(), if self.busy.is_none() { 34.0 } else { ui.spacing().interact_size.y }), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(text) = size_text { ui.label(text); }
                if self.busy.is_none() {
                    ui.spacing_mut().button_padding = vec2(12.0, 8.0);
                    retry = ui.add(ghost_button("Retry")).clicked();
                }
        });
        if retry { if let Some(job) = self.retry.clone() { self.start_download(job); } }
    }

    fn settings_panel(&mut self, ctx: &egui::Context) {
        if !self.settings_open { return; }
        let mut open = true;
        let before = serde_json::to_string(&self.settings).unwrap_or_default();
        let mut check = false;
        let mut download = false;
        let mut restart = false;
        let mut folder = false;
        let response = egui::Modal::new(Id::new("settings-popup"))
            .frame(egui::Frame::popup(&ctx.style()).inner_margin(22.0).corner_radius(14.0))
            .show(ctx, |ui| {
                ui.set_width(480.0);
                ui.style_mut().text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
                ui.style_mut().text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
                ui.style_mut().text_styles.insert(egui::TextStyle::Heading, egui::FontId::proportional(20.0));
                ui.spacing_mut().item_spacing.y = 10.0;
                ui.spacing_mut().button_padding = vec2(12.0, 8.0);
                let (header, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
                ui.painter().text(header.center(), Align2::CENTER_CENTER, "Settings", egui::FontId::proportional(20.0), TEXT);
                let close = Rect::from_center_size(pos2(header.right() - 14.0, header.center().y), vec2(28.0, 28.0));
                if window_button(ui, "settings-close", close, WindowBtn::Close) { open = false; }
                ui.add_space(4.0);
                ui.checkbox(&mut self.settings.check_on_startup, "Check app updates on startup");
                ui.checkbox(&mut self.settings.check_launcher_on_startup, "Check launcher updates on startup");
                ui.checkbox(&mut self.settings.close_to_tray, "Close button hides to system tray");
                ui.checkbox(&mut self.settings.beta_updates, "Include beta launcher releases")
                    .on_hover_text("Include prerelease versions of CraftLauncher. Turn off to receive stable releases only.");
                let settings_button = |label: &str| Button::new(RichText::new(label.to_owned()).size(15.0)).min_size(vec2(0.0, 34.0)).corner_radius(8.0);
                folder = ui.add(settings_button("Open launcher data folder")).clicked();
                ui.separator();
                let button_width = 198.0;
                let text_width = (ui.available_width() - button_width - ui.spacing().item_spacing.x).max(80.0);
                let title = ui.painter().layout(format!("CraftLauncher v{}", env!("CARGO_PKG_VERSION")),
                    egui::TextStyle::Body.resolve(ui.style()), TEXT, text_width);
                let status = ui.painter().layout(self.launcher_status.clone(),
                    egui::TextStyle::Body.resolve(ui.style()), TEXT, text_width);
                let text_height = title.size().y + 4.0 + status.size().y;
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), text_height.max(34.0)), Sense::hover());
                let text_top = pos2(row.left(), row.center().y - text_height * 0.5);
                let status_top = text_top + vec2(0.0, title.size().y + 4.0);
                ui.painter().galley(text_top, title, TEXT);
                ui.painter().galley(status_top, status, TEXT);
                let button_rect = Rect::from_center_size(pos2(row.right() - button_width * 0.5, row.center().y), vec2(button_width, 34.0));
                check = placed_button(ui, button_rect, !self.launcher_checking, settings_button("Check launcher updates")).clicked();
                if let Some(release) = &self.launcher_release {
                    let size = if release.size > 0 { transfer::size(release.size) } else { "Size available during download".into() };
                    ui.label(RichText::new(format!("v{} available · {size}", release.version.trim_start_matches('v'))).color(UPDATE_TEXT));
                    ui.hyperlink_to("View release on GitHub", format!("https://github.com/yyyyynx/craft-launcher/releases/tag/{}", release.version));
                    ScrollArea::vertical().max_height(180.0).show(ui, |ui| { ui.label(&release.notes); });
                    download = ui.add_enabled(self.busy.is_none(), settings_button("Download launcher update")).clicked();
                }
                if self.launcher_ready.is_some() {
                    ui.label("Update verified and ready. Your installed apps will be kept.");
                    restart = ui.add_enabled(self.busy.is_none(), settings_button("Restart and update")).clicked();
                }
                ui.separator();
                let can_uninstall = self.busy.is_none() && APPS.iter().any(|app| update::is_installed(&self.root, app.id));
                ui.allocate_ui_with_layout(vec2(ui.available_width(), 34.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if ui.add_enabled(can_uninstall, uninstall_button_with_font("Uninstall All Apps", 15.0).min_size(vec2(0.0, 34.0))).clicked() {
                        self.uninstall_target = Some(UninstallTarget::All);
                    }
                });
            });
        self.settings_open = open && !(self.uninstall_target.is_none() && response.should_close());
        if before != serde_json::to_string(&self.settings).unwrap_or_default() {
            if let Err(e) = self.settings.save(&self.root) { self.report_error(e); }
        }
        if folder { if let Err(e) = update::open_folder(&self.root) { self.report_error(e); } }
        if check { self.launcher_checking = true; self.launcher_status = "Checking...".into(); spawn_launcher_check(self.tx.clone(), self.settings.beta_updates); }
        if download { if let Some(r) = self.launcher_release.clone() { self.start_download(DownloadJob::Launcher(r)); } }
        if restart { if let Some(path) = &self.launcher_ready {
            match launcher_update::restart_and_replace(path, &self.root) {
                Ok(()) => {
                    #[cfg(windows)]
                    if let Some(tray) = &self.tray { tray.allow_exit(); }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Err(e) => self.report_error(e),
            }
        } }
    }

    fn sidebar_header(&mut self, ui: &mut egui::Ui, rect: Rect, is_maximized: bool) {
        let response = ui.interact(rect, Id::new("sidebar-drag"), Sense::click_and_drag());
        if response.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if response.double_clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
        }

        let logo_rect = Rect::from_center_size(pos2(rect.left() + 20.0_f32, rect.center().y), vec2(28.0_f32, 28.0_f32));
        ui.painter().image(
            self.logo.id(),
            logo_rect,
            Rect::from_min_max(pos2(0.0_f32, 0.0_f32), pos2(1.0_f32, 1.0_f32)),
            Color32::WHITE,
        );
        ui.painter().text(
            pos2(logo_rect.right() + 10.0_f32, rect.center().y),
            Align2::LEFT_CENTER,
            concat!("CraftLauncher v", env!("CARGO_PKG_VERSION")),
            egui::FontId::new(16.5_f32, FontFamily::Proportional),
            TEXT,
        );
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, places: &Places, is_maximized: bool) {
        // Draggable area in top bar
        let drag_resp = ui.interact(places.top_drag, Id::new("top-bar-drag"), Sense::click_and_drag());
        if drag_resp.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag_resp.double_clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
        }

        // Search bar
        self.search_bar(ui, places.search);

        // Window controls
        if window_button(ui, "title-min", places.minimize, WindowBtn::Minimize) {
            if let Some(hwnd) = self.glass_hwnd {
                unsafe { ShowWindow(hwnd, 6); }
            }
        }
        if window_button(ui, "title-max", places.maximize, WindowBtn::Maximize(is_maximized)) {
            if let Some(hwnd) = self.glass_hwnd {
                unsafe { ShowWindow(hwnd, if IsZoomed(hwnd) != 0 { 9 } else { 3 }); }
            }
        }
        if window_button(ui, "title-close", places.close, WindowBtn::Close) {
            #[cfg(windows)]
            if self.settings.close_to_tray {
                if let Some(tray) = &self.tray {
                    tray.hide();
                    return;
                }
            }
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn search_bar(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let hovered = ui.rect_contains_pointer(rect);
        let bg_alpha = if hovered || !self.search.is_empty() { 22 } else { 14 };
        let border_alpha = if hovered || !self.search.is_empty() { 32 } else { 18 };

        ui.painter().rect_filled(rect, 12.0_f32, Color32::from_white_alpha(bg_alpha));
        ui.painter().rect_stroke(
            rect,
            12.0_f32,
            Stroke::new(1.0_f32, Color32::from_white_alpha(border_alpha)),
            StrokeKind::Inside,
        );

        // Magnifying glass icon:
        let icon_cx = rect.left() + 20.0_f32;
        let icon_cy = rect.center().y - 0.8_f32;
        let icon_center = pos2(icon_cx, icon_cy);
        ui.painter().circle_stroke(icon_center, 5.5_f32, Stroke::new(1.4_f32, MUTED));
        ui.painter().line_segment(
            [icon_center + vec2(3.8_f32, 3.8_f32), icon_center + vec2(7.2_f32, 7.2_f32)],
            Stroke::new(1.5_f32, MUTED),
        );

        // TextEdit input:
        let font_id = egui::FontId::proportional(14.5_f32);
        let row_height = ui.fonts(|f| f.row_height(&font_id));
        let clear_w = if self.search.is_empty() { 0.0_f32 } else { 26.0_f32 };
        let text_x = icon_cx + 16.0_f32;
        let text_w = (rect.right() - clear_w - text_x - 8.0_f32).max(50.0_f32);
        let text_y = rect.center().y - row_height * 0.5_f32;
        let text_rect = Rect::from_min_size(pos2(text_x, text_y), vec2(text_w, row_height));

        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(text_rect), |ui| {
            ui.add(
                TextEdit::singleline(&mut self.search)
                    .hint_text("Search craft apps...")
                    .frame(false)
                    .margin(egui::Margin::ZERO)
                    .desired_width(text_w)
                    .text_color(TEXT)
                    .font(font_id),
            );
        });

        // Clear button
        if !self.search.is_empty() {
            let clear_rect = Rect::from_center_size(pos2(rect.right() - 16.0_f32, rect.center().y), vec2(20.0_f32, 20.0_f32));
            let clear_resp = ui.interact(clear_rect, Id::new("search-clear"), Sense::click());
            let clear_color = if clear_resp.hovered() { TEXT } else { MUTED };
            let d = 3.5_f32;
            let c = clear_rect.center();
            ui.painter().line_segment([c + vec2(-d, -d), c + vec2(d, d)], Stroke::new(1.3_f32, clear_color));
            ui.painter().line_segment([c + vec2(d, -d), c + vec2(-d, d)], Stroke::new(1.3_f32, clear_color));
            if clear_resp.clicked() {
                self.search.clear();
            }
            clear_resp.on_hover_cursor(CursorIcon::PointingHand);
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, _side: Rect, nav: Rect, update_all: Rect) {
        let filters = [
            Filter::All,
            Filter::Updates,
            Filter::Recent,
            Filter::Favorites,
            Filter::Group(Group::Image),
            Filter::Group(Group::Video),
            Filter::Group(Group::Design),
            Filter::Group(Group::Documents),
        ];
        let mut y = nav.top();
        for (index, filter) in filters.iter().copied().enumerate() {
            if index == 4 {
                ui.painter().text(
                    pos2(nav.left() + 10.0_f32, y + 10.0_f32),
                    Align2::LEFT_CENTER,
                    "CATEGORIES",
                    egui::FontId::new(10.5_f32, FontFamily::Proportional),
                    Color32::from_white_alpha(100),
                );
                y += 24.0_f32;
            }
            let row = Rect::from_min_size(pos2(nav.left(), y), vec2(nav.width(), 36.0_f32));
            if row.bottom() > nav.bottom() {
                break;
            }
            self.nav_row(ui, row, filter);
            y += 40.0_f32;
        }

        let enabled = self.busy.is_none() && !self.checking;
        let updates = self.update_count();
        let label = if self.busy.as_deref() == Some("*") {
            "Updating..."
        } else if self.checking {
            "Checking..."
        } else if updates > 0 {
            "Update all"
        } else {
            "Check for updates"
        };
        let response = ui.put(
            update_all,
            Button::new(RichText::new(label).size(15.0_f32).color(Color32::WHITE))
                .fill(if enabled { ACCENT } else { Color32::from_rgba_unmultiplied(124, 108, 242, 120) })
                .min_size(update_all.size())
                .corner_radius(10.0_f32),
        );
        if enabled && response.clicked() {
            if updates > 0 {
                self.start_all();
            } else {
                self.start_check();
            }
        }
        response.on_hover_cursor(if enabled { CursorIcon::PointingHand } else { CursorIcon::NotAllowed });
        let settings_rect = Rect::from_min_max(
            pos2(update_all.left(), update_all.top() - 44.0_f32),
            pos2(update_all.right(), update_all.top() - 8.0_f32),
        );
        let settings_label = if self.launcher_release.is_some() { "Settings · Launcher update" } else { "Settings" };
        if ui.put(settings_rect, Button::new(RichText::new(settings_label).size(15.0)).corner_radius(10.0)).clicked() { self.settings_open = true; }
    }

    fn nav_row(&mut self, ui: &mut egui::Ui, rect: Rect, filter: Filter) {
        let response = ui.interact(rect, Id::new(("nav", nav_name(filter))), Sense::click());
        let selected = self.filter == filter;
        if selected || response.hovered() {
            let alpha = if selected { 26 } else { 12 };
            ui.painter().rect_filled(rect, 10.0_f32, Color32::from_white_alpha(alpha));
        }
        let color = if selected { Color32::WHITE } else { Color32::from_rgb(190, 186, 210) };
        let icon = Rect::from_center_size(pos2(rect.left() + 18.0_f32, rect.center().y), vec2(16.0_f32, 16.0_f32));
        paint_nav_icon(ui.painter(), icon, filter, color);
        ui.painter().text(
            pos2(rect.left() + 38.0_f32, rect.center().y),
            Align2::LEFT_CENTER,
            nav_name(filter),
            egui::FontId::new(14.5_f32, FontFamily::Proportional),
            color,
        );
        ui.painter().text(
            pos2(rect.right() - 12.0_f32, rect.center().y),
            Align2::RIGHT_CENTER,
            self.nav_count(filter).to_string(),
            egui::FontId::new(12.5_f32, FontFamily::Proportional),
            if selected { Color32::from_white_alpha(140) } else { Color32::from_white_alpha(75) },
        );
        if response.clicked() {
            self.filter = filter;
        }
        response.on_hover_cursor(CursorIcon::PointingHand);
    }

    fn grid(&mut self, ui: &mut egui::Ui, rect: Rect) {
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
            let apps = self.visible();
            if apps.is_empty() {
                ui.add_space(32.0_f32);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(empty_message(self.filter)).size(15.0_f32).color(MUTED));
                });
                return;
            }
            ScrollArea::vertical().id_salt("app-grid").show(ui, |ui| {
                // 5 icons per row filling edge-to-edge as requested!
                let gap_x = 10.0_f32;
                let cols = 5;
                let col_w = ((ui.available_width() - (cols - 1) as f32 * gap_x) / cols as f32).max(110.0_f32);
                let icon_size = (col_w * 0.65_f32).clamp(96.0_f32, 210.0_f32);
                let cell_h = icon_size + 68.0_f32;

                let mut index = 0;
                while index < apps.len() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(gap_x, 0.0_f32);
                        for column in 0..cols {
                            let slot = index + column;
                            if slot < apps.len() {
                                let (cell, _) = ui.allocate_exact_size(vec2(col_w, cell_h), Sense::hover());
                                self.tile(ui, cell, apps[slot], icon_size);
                            }
                        }
                    });
                    index += cols;
                }
            });
        });
    }

    fn tile(&mut self, ui: &mut egui::Ui, cell: Rect, index: usize, icon_size: f32) {
        let app = &APPS[index];
        let id = app.id;
        let name = app.name;
        let selected = self.selected == Some(index);
        let has_upd = self.has_update(id);

        let name_font_size = (14.5_f32 + (icon_size - 100.0_f32) * 0.022_f32).clamp(14.0_f32, 17.0_f32);
        let ver_font_size = (12.5_f32 + (icon_size - 100.0_f32) * 0.018_f32).clamp(12.0_f32, 14.5_f32);

        // Equal padding (13.0px) on both top and bottom
        let pad = 13.0_f32;
        let card_h = icon_size + 62.0_f32;
        let card_rect = Rect::from_min_size(pos2(cell.left(), cell.top()), vec2(cell.width(), card_h));
        let hovered = ui.rect_contains_pointer(card_rect);

        if selected || hovered {
            let fill = if selected {
                Color32::from_white_alpha(18)
            } else {
                Color32::from_white_alpha(8)
            };
            ui.painter().rect_filled(card_rect, 14.0_f32, fill);
            let border_stroke = if selected {
                Stroke::new(1.2_f32, Color32::from_rgba_unmultiplied(124, 108, 242, 180))
            } else {
                Stroke::new(1.0_f32, Color32::from_white_alpha(14))
            };
            ui.painter().rect_stroke(card_rect, 14.0_f32, border_stroke, StrokeKind::Inside);
        }

        let icon_rect = Rect::from_center_size(
            pos2(card_rect.center().x, card_rect.top() + pad + icon_size * 0.5_f32),
            vec2(icon_size, icon_size),
        );
        let response = ui.interact(card_rect, Id::new(("tile", id)), Sense::click());

        let installed_on_disk = update::is_installed(&self.root, id);
        let icons = if installed_on_disk { &self.icons } else { &self.uninstalled_icons };
        if let Some(texture) = icons.get(id) {
            ui.painter().image(texture.id(), icon_rect, Rect::from_min_max(pos2(0.0_f32, 0.0_f32), pos2(1.0_f32, 1.0_f32)), Color32::WHITE);
        } else {
            ui.painter().rect_filled(icon_rect, 18.0_f32, if installed_on_disk { fallback_color(id) } else { Color32::GRAY });
        }

        if has_upd {
            let dot = icon_rect.right_top() + vec2(-4.0_f32, 6.0_f32);
            let dot_radius = (7.0_f32 + (icon_size - 100.0_f32) * 0.015_f32).clamp(7.0_f32, 8.5_f32);
            ui.painter().circle_filled(dot, dot_radius, ACCENT);
            ui.painter().circle_stroke(dot, dot_radius, Stroke::new(2.2_f32, Color32::WHITE));
        }

        let favorite = self.favorites.iter().any(|favorite| favorite == id);
        let star_scale = (icon_size / 116.0).clamp(0.85, 1.85);
        let star_rect = Rect::from_center_size(icon_rect.left_top() + vec2(10.0, 10.0) * star_scale, vec2(28.0, 28.0) * star_scale);
        let star_response = ui.interact(star_rect, Id::new(("favorite", id)), Sense::click());
        ui.painter().circle_filled(star_rect.center(), 11.0 * star_scale, Color32::from_rgb(38, 34, 48));
        paint_star(ui.painter(), star_rect.center(), if favorite { Color32::from_rgb(255, 215, 99) } else { TEXT }, favorite, 16.0 * star_scale);
        if star_response.clicked() { self.toggle_favorite(id); }
        let star_hovered = star_response.hovered();
        star_response.on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(if favorite { "Remove from Favorites" } else { "Add to Favorites" });

        let name_y = icon_rect.bottom() + 13.0_f32;
        ui.painter().text(
            pos2(card_rect.center().x, name_y),
            Align2::CENTER_CENTER,
            name,
            egui::FontId::new(name_font_size, FontFamily::Proportional),
            TEXT,
        );

        let version_y = name_y + 17.0_f32;
        let version = self.installed.get(id).map(String::as_str).unwrap_or("");
        let version_text = if version.is_empty() {
            "Not installed".to_string()
        } else {
            version.to_string()
        };
        let mut version_job = egui::text::LayoutJob::default();
        let version_format = |color| egui::TextFormat {
            font_id: egui::FontId::new(ver_font_size, FontFamily::Proportional),
            color, ..Default::default()
        };
        version_job.append(&version_text, 0.0, version_format(if version.is_empty() { MUTED } else { TEXT }));
        if has_upd && !version.is_empty() {
            if let Some(remote) = self.remote.get(id) {
                version_job.append(" -> ", 0.0, version_format(TEXT));
                version_job.append(&remote.tag, 0.0, version_format(UPDATE_TEXT));
            }
        }
        let version_galley = ui.painter().layout_job(version_job);
        ui.painter().galley(pos2(card_rect.center().x, version_y) - version_galley.size() * 0.5, version_galley, TEXT);

        if response.clicked() && !star_hovered {
            self.selected = if self.selected == Some(index) { None } else { Some(index) };
        }
        if response.double_clicked() && !star_hovered {
            self.open_index(index);
        }
        response.on_hover_cursor(CursorIcon::PointingHand);

    }

    fn app_details(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(index) = self.selected else { return false };
        let mut install_requested = false;
        let app = &APPS[index];
        let id = app.id.to_string();
        let name = app.name.to_string();
        let description = app.description;
        let installed = self.installed.get(&id).cloned().unwrap_or_default();
        let remote = self.remote.get(&id).cloned();
        let logs = self.logs.get(&id).cloned().unwrap_or_default();
        let busy = self.busy.is_some();

        let detail_button = |label: &str, fill: Color32| {
            Button::new(RichText::new(label.to_owned()).size(16.0_f32).color(TEXT))
                .fill(fill).corner_radius(12.0_f32)
        };
        let response = egui::Frame::NONE
            .fill(Color32::from_rgba_unmultiplied(35, 30, 49, 248))
            .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(20)))
            .corner_radius(20.0_f32)
            .inner_margin(24.0_f32)
            .show(ui, |ui| {
            ui.set_width(512.0_f32);
            ui.spacing_mut().item_spacing.x = 12.0_f32;
            ui.horizontal(|ui| {
                let (icon_rect, _) = ui.allocate_exact_size(vec2(64.0_f32, 64.0_f32), Sense::hover());
                let icons = if update::is_installed(&self.root, &id) { &self.icons } else { &self.uninstalled_icons };
                if let Some(texture) = icons.get(&id) {
                    ui.painter().image(
                        texture.id(),
                        icon_rect,
                        Rect::from_min_max(pos2(0.0_f32, 0.0_f32), pos2(1.0_f32, 1.0_f32)),
                        Color32::WHITE,
                    );
                } else {
                    ui.painter().rect_filled(icon_rect, 10.0_f32, fallback_color(&id));
                }
                ui.add_space(12.0_f32);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.add(Label::new(RichText::new(&name).size(24.0_f32).strong().color(TEXT)).selectable(false));
                        if !installed.is_empty() {
                            ui.label(RichText::new(&installed).size(14.0).color(Color32::from_rgb(196, 192, 208)));
                        }
                    });
                    ui.label(RichText::new(description).size(14.0_f32).color(MUTED));
                    ui.horizontal(|ui| {
                        let link_text = |label: &str| RichText::new(label).size(14.0_f32).color(Color32::from_rgb(186, 176, 255));
                        ui.hyperlink_to(link_text("GitHub"), format!("https://github.com/storytold/{id}"));
                        ui.hyperlink_to(link_text("Releases"), format!("https://github.com/storytold/{id}/releases"));
                        ui.hyperlink_to(link_text("Website"), format!("https://getartcraft.com/apps/{id}"));
                    });
                });
            });
            ui.add_space(14.0_f32);
            if installed.is_empty() {
                ui.label(RichText::new("Not installed").size(15.0_f32).color(MUTED));
            }
            if let Some(remote) = &remote {
                if remote.tag != installed {
                    ui.label(RichText::new(format!("{} available", remote.tag)).size(15.0_f32).color(UPDATE_TEXT));
                }
            }
            ui.add_space(12.0_f32);
            let gap = ui.spacing().item_spacing.x;
            let (actions_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0_f32), Sense::hover());
            let button_width = (actions_rect.width() - gap * 2.0_f32) / 3.0_f32;
            let button_rect = |column: usize| Rect::from_min_size(
                actions_rect.min + vec2(column as f32 * (button_width + gap), 0.0_f32),
                vec2(button_width, actions_rect.height()),
            );
            let can_manage = !busy && update::is_installed(&self.root, &id);
            let open_rect = button_rect(0);
            if placed_button(ui, open_rect, can_manage, detail_button("Open", Color32::from_white_alpha(18))).clicked() {
                if self.open_index(index) { self.selected = None; }
            }
            let update_rect = button_rect(1);
            let installed_on_disk = update::is_installed(&self.root, &id);
            let can_update = !busy && (!installed_on_disk || self.has_update(&id));
            let update_label = if installed_on_disk { "Update" } else { "Install" };
            if placed_button(ui, update_rect, can_update, detail_button(update_label, ACCENT)).clicked() {
                install_requested = true;
            }
            let uninstall_rect = button_rect(2);
            if placed_button(ui, uninstall_rect, can_manage, detail_button("Uninstall", Color32::from_rgb(151, 49, 65))).clicked() {
                self.uninstall_target = Some(UninstallTarget::App(index));
            }
            ui.add_space(20.0_f32);
            if self.retry.is_some() { self.download_controls(ui); ui.add_space(12.0); }
            if can_manage {
                ui.horizontal(|ui| {
                    if let Some(Ok(size)) = self.usage.get(&id) {
                        let text = ui.painter().layout_no_wrap(format!("Installed size: {}", transfer::size(*size)),
                            egui::TextStyle::Body.resolve(ui.style()), TEXT);
                        let (rect, _) = ui.allocate_exact_size(vec2(text.size().x, 22.0), Sense::hover());
                        ui.painter().galley(pos2(rect.left(), rect.bottom() - text.size().y), text, TEXT);
                    }
                    ui.allocate_ui_with_layout(vec2(ui.available_width(), 22.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().button_padding = vec2(7.0, 3.0);
                        ui.spacing_mut().interact_size.y = 22.0;
                        let badge = |label: &str| Button::new(RichText::new(label.to_owned()).size(12.0).color(TEXT))
                            .fill(Color32::from_white_alpha(18)).min_size(vec2(0.0, 22.0)).corner_radius(6.0);
                        if ui.add(badge("Repair / Reinstall")).clicked() { self.start_download(DownloadJob::App { id: id.clone(), repair: true }); }
                        if ui.add(badge("Open app folder")).clicked() {
                            if let Err(e) = update::open_folder(&update::app_folder(&self.root, &id)) { self.report_error(e); }
                        }
                    });
                });
                ui.add_space(8.0);
            }
            let (log_rect, log_response) = ui.allocate_exact_size(vec2(ui.available_width(), 54.0_f32), Sense::click());
            let log_fill = if log_response.hovered() {
                Color32::from_white_alpha(28)
            } else {
                Color32::from_white_alpha(14)
            };
            ui.painter().rect_filled(log_rect, 12.0_f32, log_fill);
            ui.painter().rect_stroke(log_rect, 12.0_f32, Stroke::new(1.0_f32, Color32::from_white_alpha(24)), StrokeKind::Inside);
            ui.painter().text(
                pos2(log_rect.left() + 14.0_f32, log_rect.center().y),
                Align2::LEFT_CENTER,
                "Update log",
                egui::FontId::new(16.0_f32, FontFamily::Proportional),
                TEXT,
            );
            let count = if logs.is_empty() { "None yet".to_string() } else { format!("{} entries", logs.len()) };
            let count_font = egui::FontId::new(15.0_f32, FontFamily::Proportional);
            let count_size = ui.painter().layout_no_wrap(count.clone(), count_font.clone(), TEXT).size();
            let count_rect = Rect::from_center_size(
                pos2(log_rect.right() - 14.0_f32 - (count_size.x + 24.0_f32) * 0.5_f32, log_rect.center().y),
                vec2(count_size.x + 24.0_f32, 30.0_f32),
            );
            ui.painter().rect_filled(count_rect, 8.0_f32, Color32::from_rgb(46, 39, 66));
            ui.painter().text(
                count_rect.center(),
                Align2::CENTER_CENTER,
                count,
                count_font,
                TEXT,
            );
            if log_response.clicked() {
                self.patch_app = Some(index);
            }
            log_response.on_hover_cursor(CursorIcon::PointingHand);
        });

        let rect = response.response.rect;
        let close = Rect::from_center_size(pos2(rect.right() - 24.0_f32, rect.top() + 24.0_f32), vec2(30.0_f32, 30.0_f32));
        if window_button(ui, "details-close", close, WindowBtn::Close) {
            self.selected = None;
        }
        install_requested
    }

    fn patch_popup(&mut self, ui: &mut egui::Ui, screen: Rect) {
        let Some(index) = self.patch_app else { return };
        if index >= APPS.len() {
            self.patch_app = None;
            return;
        }
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.patch_app = None;
            return;
        }

        let dim = ui.interact(screen, Id::new("patch-dim"), Sense::click());
        ui.painter().rect_filled(screen, 0.0_f32, Color32::from_black_alpha(150));

        let size = vec2(640.0_f32, 560.0_f32).min(screen.size() - vec2(64.0_f32, 64.0_f32));
        let card = Rect::from_center_size(screen.center(), size);
        let _card_hit = ui.interact(card, Id::new("patch-card"), Sense::click());
        ui.painter().rect_filled(card, 20.0_f32, Color32::from_rgba_unmultiplied(24, 20, 38, 248));
        ui.painter().rect_stroke(card, 20.0_f32, Stroke::new(1.0_f32, Color32::from_white_alpha(28)), StrokeKind::Inside);

        let app = &APPS[index];
        let id = app.id.to_string();
        let name = app.name.to_string();
        let logs = self.logs.get(&id).cloned().unwrap_or_default();
        let remote = self.remote.get(&id).cloned();
        let installed = self.installed.get(&id).cloned().unwrap_or_default();

        let header = Rect::from_min_max(card.min + vec2(22.0_f32, 16.0_f32), pos2(card.max.x - 22.0_f32, card.min.y + 78.0_f32));
        let icon_rect = Rect::from_min_size(header.min, vec2(44.0_f32, 44.0_f32));
        if let Some(texture) = self.icons.get(&id) {
            ui.painter().image(texture.id(), icon_rect, Rect::from_min_max(pos2(0.0_f32, 0.0_f32), pos2(1.0_f32, 1.0_f32)), Color32::WHITE);
        }
        ui.painter().text(
            pos2(icon_rect.right() + 12.0_f32, header.top() + 14.0_f32),
            Align2::LEFT_CENTER,
            &name,
            egui::FontId::new(20.0_f32, FontFamily::Proportional),
            TEXT,
        );
        ui.painter().text(
            pos2(icon_rect.right() + 12.0_f32, header.top() + 36.0_f32),
            Align2::LEFT_CENTER,
            "Patch notes",
            egui::FontId::new(13.0_f32, FontFamily::Proportional),
            MUTED,
        );
        let close = Rect::from_center_size(pos2(header.right() - 8.0_f32, header.top() + 16.0_f32), vec2(28.0_f32, 28.0_f32));
        let close_hit = window_button(ui, "patch-close", close, WindowBtn::Close);

        let body = Rect::from_min_max(pos2(card.left() + 22.0_f32, header.bottom() + 4.0_f32), card.max - vec2(22.0_f32, 18.0_f32));
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(body), |ui| {
            ScrollArea::vertical().id_salt(("patch", id.clone())).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let mut any = false;
                if let Some(remote) = &remote {
                    if remote.tag != installed {
                        any = true;
                        patch_section(ui, "Available", &remote.tag, &remote.title, &remote.body, true);
                    }
                }
                if logs.is_empty() && !any {
                    ui.add_space(24.0_f32);
                    ui.label(RichText::new("No updates recorded yet").size(14.0_f32).color(MUTED));
                }
                for entry in &logs {
                    patch_section(ui, &entry.at, &entry.title, "", &entry.body, false);
                }
            });
        });

        if close_hit || dim.clicked() {
            self.patch_app = None;
        }
    }

    fn status_line(&self, ui: &mut egui::Ui, rect: Rect) {
        let dot = pos2(rect.left() + 6.0_f32, rect.center().y);
        let dot_color = if self.busy.is_some() || self.checking {
            ACCENT
        } else if self.update_count() > 0 {
            Color32::from_rgb(240, 160, 48)
        } else {
            Color32::from_rgb(46, 204, 113)
        };
        ui.painter().circle_filled(dot, 3.5_f32, dot_color);
        ui.painter().text(
            pos2(dot.x + 10.0_f32, rect.center().y),
            Align2::LEFT_CENTER,
            &self.status,
            egui::FontId::new(12.5_f32, FontFamily::Proportional),
            MUTED,
        );
        paint_thanks(ui, rect);
    }
}

fn patch_section(ui: &mut egui::Ui, when: &str, title: &str, subtitle: &str, body: &str, accent: bool) {
    ui.add_space(8.0_f32);
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0_f32), Sense::hover());
    let chip = if accent { ACCENT } else { Color32::from_white_alpha(16) };
    let chip_text = TEXT;
    let label = if title.is_empty() { when.to_string() } else { title.to_string() };
    ui.painter().text(pos2(bar.left(), bar.center().y), Align2::LEFT_CENTER, label, egui::FontId::new(15.5_f32, FontFamily::Proportional), TEXT);
    if !when.is_empty() && !title.is_empty() {
        let (when, inferred) = log_date(when, chrono::Local::now().date_naive());
        let when_w = 116.0_f32;
        let when_rect = Rect::from_min_size(pos2(bar.right() - when_w - 20.0_f32, bar.center().y - 11.0_f32), vec2(when_w, 22.0_f32));
        ui.painter().rect_filled(when_rect, 11.0_f32, chip);
        ui.painter().text(when_rect.center(), Align2::CENTER_CENTER, when, egui::FontId::new(13.0_f32, FontFamily::Proportional), chip_text);
        if inferred {
            ui.interact(when_rect, ui.id().with(("legacy-log-date", bar.top().to_bits())), Sense::hover())
                .on_hover_text("This older log did not record a year. The year is estimated from the most recent occurrence of this date.");
        }
    }
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).size(13.0_f32).color(MUTED));
    }
    let bullets = patch_bullets(body);
    if bullets.is_empty() {
        let paragraph = body.trim();
        if !paragraph.is_empty() && !paragraph.contains('\n') {
            ui.label(RichText::new(paragraph).size(13.5_f32).color(MUTED));
        }
    } else {
        ui.add_space(4.0_f32);
        for bullet in bullets {
            ui.add_space(3.0_f32);
            ui.label(RichText::new(format!("•  {bullet}")).size(14.0_f32).color(Color32::from_rgb(232, 228, 242)));
        }
    }
    ui.add_space(10.0_f32);
    let y = ui.cursor().top();
    ui.painter().line_segment(
        [pos2(ui.min_rect().left(), y), pos2(ui.max_rect().right(), y)],
        Stroke::new(1.0_f32, Color32::from_white_alpha(16)),
    );
}

fn log_date(when: &str, today: chrono::NaiveDate) -> (String, bool) {
    use chrono::Datelike;
    if let Ok(date) = chrono::DateTime::parse_from_rfc3339(when) {
        return (date.format("%d %b %Y").to_string(), false);
    }
    for format in ["%d %b %Y", "%d %b %Y %H:%M", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(date) = chrono::NaiveDate::parse_from_str(when, format) {
            return (date.format("%d %b %Y").to_string(), false);
        }
    }
    let date = when.split_whitespace().filter(|part| !part.contains(':')).collect::<Vec<_>>().join(" ");
    for year in (today.year() - 4..=today.year()).rev() {
        if let Ok(candidate) = chrono::NaiveDate::parse_from_str(&format!("{date} {year}"), "%d %b %Y") {
            if candidate <= today { return (candidate.format("%d %b %Y").to_string(), true); }
        }
    }
    (date, false)
}

fn patch_bullets(body: &str) -> Vec<String> {
    let mut notes = Vec::new();
    for raw in body.lines() {
        let line = clean_note_line(raw);
        if line.is_empty() {
            continue;
        }
        notes.push(line);
        if notes.len() == 30 {
            break;
        }
    }
    notes
}

fn clean_note_line(raw: &str) -> String {
    let line = raw.trim();
    if line.is_empty() || line.starts_with('#') {
        return String::new();
    }
    let line = line.trim_start_matches(['*', '-', '•', '·']).trim();
    let mut text = line.to_string();
    if let Some(index) = text.find(" by @") {
        text.truncate(index);
    }
    if let Some((head, rest)) = text.split_once("  ") {
        if (7..=40).contains(&head.len()) && head.chars().all(|c| c.is_ascii_hexdigit()) {
            text = rest.trim().to_string();
        }
    }
    let chars: Vec<char> = text.chars().collect();
    let mut cleaned = String::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '[' {
            if let Some(rel_mid) = chars[index + 1..].iter().position(|c| *c == ']') {
                let mid = index + 1 + rel_mid;
                if mid + 1 < chars.len() && chars[mid + 1] == '(' {
                    if let Some(rel_end) = chars[mid + 2..].iter().position(|c| *c == ')') {
                        cleaned.extend(chars[index + 1..mid].iter());
                        index = mid + 2 + rel_end + 1;
                        continue;
                    }
                }
            }
        }
        cleaned.push(chars[index]);
        index += 1;
    }
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn paint_thanks(ui: &mut egui::Ui, rect: Rect) {
    let font = egui::FontId::new(12.0_f32, FontFamily::Proportional);
    let lead = "Special thanks to ";
    let name = "storytold";
    let tail = " · ArtCraft creator";
    let lead_size = ui.painter().layout_no_wrap(lead.to_owned(), font.clone(), MUTED).size();
    let name_size = ui.painter().layout_no_wrap(name.to_owned(), font.clone(), MUTED).size();
    let tail_size = ui.painter().layout_no_wrap(tail.to_owned(), font.clone(), MUTED).size();
    let x = rect.right() - (lead_size.x + name_size.x + tail_size.x);
    let y = rect.center().y;
    ui.painter().text(pos2(x, y), Align2::LEFT_CENTER, lead, font.clone(), MUTED);
    let name_origin = pos2(x + lead_size.x, y - name_size.y * 0.5_f32);
    let name_rect = Rect::from_min_size(name_origin, name_size);
    let response = ui.interact(name_rect, Id::new("storytold-thanks"), Sense::click());
    let name_color = if response.hovered() { Color32::WHITE } else { Color32::from_rgb(186, 176, 255) };
    ui.painter().text(name_rect.left_center(), Align2::LEFT_CENTER, name, font.clone(), name_color);
    let underline = name_rect.bottom() - 1.0_f32;
    ui.painter().line_segment(
        [pos2(name_rect.left(), underline), pos2(name_rect.right(), underline)],
        Stroke::new(1.0_f32, name_color),
    );
    ui.painter().text(pos2(name_rect.right(), y), Align2::LEFT_CENTER, tail, font, MUTED);
    if response.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab("https://github.com/storytold"));
    }
    response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(
        "Thank you for creating ArtCraft and sharing your work with the community. Visit storytold on GitHub.",
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WindowBtn {
    Minimize,
    Maximize(bool),
    Close,
}

fn window_button(ui: &mut egui::Ui, id: &str, rect: Rect, kind: WindowBtn) -> bool {
    let is_close = kind == WindowBtn::Close;
    let response = ui.interact(rect, Id::new((id, is_close)), Sense::click());
    let fill = if response.hovered() && is_close {
        Color32::from_rgb(220, 60, 70)
    } else if response.hovered() {
        Color32::from_white_alpha(36)
    } else {
        Color32::from_white_alpha(16)
    };
    ui.painter().circle_filled(rect.center(), rect.width() * 0.5_f32, fill);
    let color = Color32::WHITE;
    let center = rect.center();

    match kind {
        WindowBtn::Close => {
            let d = 4.2_f32;
            ui.painter().line_segment([center + vec2(-d, -d), center + vec2(d, d)], Stroke::new(1.4_f32, color));
            ui.painter().line_segment([center + vec2(d, -d), center + vec2(-d, d)], Stroke::new(1.4_f32, color));
        }
        WindowBtn::Minimize => {
            ui.painter().line_segment(
                [center + vec2(-4.5_f32, 0.0_f32), center + vec2(4.5_f32, 0.0_f32)],
                Stroke::new(1.5_f32, color),
            );
        }
        WindowBtn::Maximize(is_maximized) => {
            if is_maximized {
                let back = Rect::from_center_size(center + vec2(2.0_f32, -2.0_f32), vec2(7.5_f32, 7.5_f32));
                ui.painter().rect_stroke(back, 1.0_f32, Stroke::new(1.2_f32, color), StrokeKind::Inside);
                let front = Rect::from_center_size(center + vec2(-2.0_f32, 2.0_f32), vec2(7.5_f32, 7.5_f32));
                ui.painter().rect_filled(front, 1.0_f32, fill);
                ui.painter().rect_stroke(front, 1.0_f32, Stroke::new(1.2_f32, color), StrokeKind::Inside);
            } else {
                let box_rect = Rect::from_center_size(center, vec2(9.5_f32, 9.5_f32));
                ui.painter().rect_stroke(box_rect, 1.2_f32, Stroke::new(1.3_f32, color), StrokeKind::Inside);
            }
        }
    }
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

fn ghost_button(text: &str) -> Button<'_> {
    ghost_button_with_font(text, 14.0)
}

fn ghost_button_with_font(text: &str, size: f32) -> Button<'_> {
    Button::new(RichText::new(text).size(size).color(TEXT))
        .fill(Color32::from_white_alpha(18))
        .min_size(vec2(84.0_f32, 34.0_f32))
        .corner_radius(10.0_f32)
}

fn uninstall_button_with_font(text: &str, size: f32) -> Button<'_> {
    Button::new(RichText::new(text).size(size).color(Color32::WHITE))
        .fill(Color32::from_rgb(151, 49, 65))
        .min_size(vec2(90.0_f32, 34.0_f32))
        .corner_radius(10.0_f32)
}

fn placed_button(ui: &mut egui::Ui, rect: Rect, enabled: bool, button: Button<'_>) -> egui::Response {
    ui.allocate_new_ui(
        egui::UiBuilder::new().max_rect(rect)
            .layout(egui::Layout::centered_and_justified(egui::Direction::TopDown)),
        |ui| ui.add_enabled(enabled, button.min_size(rect.size())),
    ).inner
}

fn nav_name(filter: Filter) -> &'static str {
    match filter {
        Filter::All => "All Apps",
        Filter::Updates => "Updates",
        Filter::Recent => "Recent",
        Filter::Favorites => "Favorites",
        Filter::Group(group) => group.label(),
    }
}

fn empty_message(filter: Filter) -> &'static str {
    match filter {
        Filter::Updates => "Everything is up to date",
        Filter::Recent => "No recent apps",
        Filter::Favorites => "No favorites yet. Click a star to add an app.",
        _ => "No apps found",
    }
}

fn paint_nav_icon(painter: &egui::Painter, rect: Rect, filter: Filter, color: Color32) {
    let center = rect.center();
    match filter {
        Filter::All => paint_grid_icon(painter, center, color),
        Filter::Updates => paint_download(painter, center, color),
        Filter::Recent => paint_bolt(painter, center, color),
        Filter::Favorites => paint_star(painter, center, color, true, 16.0),
        Filter::Group(Group::Image) => paint_folder(painter, center, color),
        Filter::Group(Group::Video) => paint_play(painter, center, color),
        Filter::Group(Group::Design) => paint_palette(painter, center, color),
        Filter::Group(Group::Documents) => paint_page(painter, center, color),
    }
}

fn paint_grid_icon(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let tile = 6.4_f32;
    let gap = 1.7_f32;
    let span = tile * 2.0_f32 + gap;
    let origin = center - vec2(span, span) * 0.5_f32;
    let step = tile + gap;
    for ix in 0..2 {
        for iy in 0..2 {
            let cell = Rect::from_min_size(origin + vec2(ix as f32 * step, iy as f32 * step), vec2(tile, tile));
            painter.rect_filled(cell, 2.2_f32, color);
        }
    }
}

fn paint_download(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let shaft = Rect::from_center_size(center + vec2(0.0_f32, -1.6_f32), vec2(2.6_f32, 7.4_f32));
    painter.rect_filled(shaft, 1.0_f32, color);
    let head = vec![
        center + vec2(-4.4_f32, 0.6_f32),
        center + vec2(4.4_f32, 0.6_f32),
        center + vec2(0.0_f32, 5.6_f32),
    ];
    painter.add(egui::Shape::convex_polygon(head, color, Stroke::NONE));
}

fn paint_star(painter: &egui::Painter, center: egui::Pos2, color: Color32, filled: bool, size: f32) {
    let id = Id::new(("lucide-favorite-texture", filled));
    let ctx = painter.ctx();
    let texture = ctx.data(|data| data.get_temp::<TextureHandle>(id)).unwrap_or_else(|| {
        let bytes: &[u8] = if filled { include_bytes!("../assets/icons/favorite-filled.png") }
            else { include_bytes!("../assets/icons/favorite-outline.png") };
        let image = image::load_from_memory(bytes).expect("embedded favorite icon").into_rgba8();
        let image = egui::ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize], image.as_raw());
        let texture = ctx.load_texture(format!("lucide-favorite-{filled}"), image, egui::TextureOptions::LINEAR);
        ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
        texture
    });
    painter.image(texture.id(), Rect::from_center_size(center, vec2(size, size)),
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), color);
}
fn paint_bolt(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let points = [
        vec2(1.4_f32, -7.4_f32),
        vec2(-3.2_f32, 0.4_f32),
        vec2(-0.6_f32, 0.5_f32),
        vec2(-1.6_f32, 7.4_f32),
        vec2(3.4_f32, -0.6_f32),
        vec2(0.7_f32, -0.7_f32),
    ]
    .map(|offset| center + offset);
    painter.add(egui::Shape::convex_polygon(points.to_vec(), color, Stroke::NONE));
}

fn paint_folder(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let body = Rect::from_center_size(center + vec2(0.0_f32, 1.2_f32), vec2(14.5_f32, 9.2_f32));
    painter.rect_filled(body, 2.0_f32, color);
    let tab = Rect::from_min_size(body.left_top() + vec2(1.4_f32, -3.1_f32), vec2(6.2_f32, 3.6_f32));
    painter.rect_filled(tab, 1.4_f32, color);
}

fn paint_play(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let points = vec![
        center + vec2(-3.2_f32, -5.4_f32),
        center + vec2(5.4_f32, 0.0_f32),
        center + vec2(-3.2_f32, 5.4_f32),
    ];
    painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

fn paint_palette(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    painter.circle_filled(center, 6.6_f32, color);
    painter.circle_filled(center + vec2(2.2_f32, -2.4_f32), 1.5_f32, Color32::from_rgb(22, 18, 36));
}

fn paint_page(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let page = Rect::from_center_size(center, vec2(10.5_f32, 13.5_f32));
    painter.rect_filled(page, 2.0_f32, color);
}

fn fallback_color(id: &str) -> Color32 {
    match id {
        "photocraft" => Color32::from_rgb(0x2f, 0x7b, 0xf5),
        "vectorcraft" => Color32::from_rgb(0xc7, 0x3a, 0x32),
        "filmcraft" => Color32::from_rgb(0x8b, 0x5c, 0xf6),
        "lightcraft" => Color32::from_rgb(0xe8, 0x84, 0x1a),
        "pdfcraft" => Color32::from_rgb(0x12, 0xa5, 0x8a),
        "effectcraft" => Color32::from_rgb(0xe2, 0x54, 0x9a),
        "designcraft" => Color32::from_rgb(0x7b, 0xb5, 0x1c),
        _ => Color32::GRAY,
    }
}

fn load_icons(ctx: &egui::Context, root: &std::path::Path, grayscale: bool) -> HashMap<String, TextureHandle> {
    let mut icons = HashMap::new();
    let embedded: &[(&str, &[u8])] = &[
        ("photocraft", include_bytes!("../assets/icons/photocraft.png")),
        ("vectorcraft", include_bytes!("../assets/icons/vectorcraft.png")),
        ("filmcraft", include_bytes!("../assets/icons/filmcraft.png")),
        ("lightcraft", include_bytes!("../assets/icons/lightcraft.png")),
        ("pdfcraft", include_bytes!("../assets/icons/pdfcraft.png")),
        ("effectcraft", include_bytes!("../assets/icons/effectcraft.png")),
        ("designcraft", include_bytes!("../assets/icons/designcraft.png")),
    ];
    for &(id, bytes) in embedded {
        if let Ok(image) = image::load_from_memory(bytes) {
            let image = if grayscale { image.grayscale() } else { image };
            let rgba = crop_app_icon(image.to_rgba8());
            let size = [rgba.width() as usize, rgba.height() as usize];
            let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            icons.insert(id.to_string(), ctx.load_texture(id, color, egui::TextureOptions::LINEAR));
        }
    }
    // If any icon wasn't embedded, try loading from disk
    for app in APPS {
        if icons.contains_key(app.id) {
            continue;
        }
        let path = root
            .join("repo")
            .join(app.id)
            .join("assets/app-icon/hicolor/256x256/apps")
            .join(format!("ai.storyteller.{}.png", app.id));
        let Ok(bytes) = std::fs::read(path) else { continue };
        let Ok(image) = image::load_from_memory(&bytes) else { continue };
        let image = if grayscale { image.grayscale() } else { image };
        let rgba = crop_app_icon(image.to_rgba8());
        let size = [rgba.width() as usize, rgba.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        icons.insert(app.id.to_string(), ctx.load_texture(app.id, color, egui::TextureOptions::LINEAR));
    }
    icons
}

fn load_logo(ctx: &egui::Context) -> TextureHandle {
    let rgba = logo_rgba().unwrap_or_else(|| image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0])));
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    ctx.load_texture("artlauncher", color, egui::TextureOptions::LINEAR)
}

fn window_icon() -> Option<egui::IconData> {
    let logo = image::imageops::resize(&logo_rgba()?, 110, 110, image::imageops::FilterType::Lanczos3);
    let mut image = image::RgbaImage::new(128, 128);
    image::imageops::overlay(&mut image, &logo, 9, 9);
    let width = image.width();
    let height = image.height();
    Some(egui::IconData { rgba: image.into_raw(), width, height })
}

fn logo_rgba() -> Option<image::RgbaImage> {
    let image = image::load_from_memory(include_bytes!("../assets/artlauncher.png")).ok()?.to_rgba8();
    Some(trim_transparent(image))
}

fn crop_app_icon(source: image::RgbaImage) -> image::RgbaImage {
    let width = source.width();
    let height = source.height();
    let inset = ((width.min(height) as f32) * 0.08).round().max(1.0) as u32;
    let cropped = image::imageops::crop_imm(&source, inset, inset, width - inset * 2, height - inset * 2).to_image();
    let size = 256;
    let mut resized = image::imageops::resize(&cropped, size, size, image::imageops::FilterType::Lanczos3);
    apply_round_mask(&mut resized, size as f32 * 0.26);
    resized
}

fn trim_transparent(source: image::RgbaImage) -> image::RgbaImage {
    let (width, height) = (source.width(), source.height());
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;
    for y in 0..height {
        for x in 0..width {
            if source.get_pixel(x, y)[3] > 16 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x > max_x || min_y > max_y {
        return source;
    }
    let pad = 2;
    let left = min_x.saturating_sub(pad);
    let top = min_y.saturating_sub(pad);
    let right = (max_x + 1 + pad).min(width);
    let bottom = (max_y + 1 + pad).min(height);
    image::imageops::crop_imm(&source, left, top, right - left, bottom - top).to_image()
}

fn apply_round_mask(image: &mut image::RgbaImage, radius: f32) {
    let width = image.width() as f32;
    let height = image.height() as f32;
    for y in 0..image.height() {
        for x in 0..image.width() {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let dx = if px < radius {
                radius - px
            } else if px > width - radius {
                px - (width - radius)
            } else {
                0.0
            };
            let dy = if py < radius {
                radius - py
            } else if py > height - radius {
                py - (height - radius)
            } else {
                0.0
            };
            let dist = (dx * dx + dy * dy).sqrt();
            let cover = ((radius + 0.75 - dist) / 1.5).clamp(0.0, 1.0);
            let pixel = image.get_pixel_mut(x, y);
            pixel[3] = (pixel[3] as f32 * cover).round() as u8;
        }
    }
}

#[repr(C)]
struct AccentPolicy {
    state: u32,
    flags: u32,
    gradient: u32,
    animation_id: u32,
}

#[repr(C)]
struct WindowCompositionAttributeData {
    attribute: u32,
    data: *mut std::ffi::c_void,
    size: usize,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn FindWindowW(class: *const u16, window: *const u16) -> isize;
    fn GetWindowRect(hwnd: isize, rect: *mut NativeRect) -> i32;
    fn SetWindowRgn(hwnd: isize, region: isize, redraw: i32) -> i32;
    fn ShowWindow(hwnd: isize, command: i32) -> i32;
    fn IsIconic(hwnd: isize) -> i32;
    fn IsZoomed(hwnd: isize) -> i32;
}

#[repr(C)]
struct NativeRect { left: i32, top: i32, right: i32, bottom: i32 }

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateRoundRectRgn(left: i32, top: i32, right: i32, bottom: i32, width: i32, height: i32) -> isize;
    fn DeleteObject(object: isize) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
    fn GetProcAddress(module: isize, name: *const u8) -> *const std::ffi::c_void;
}

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(hwnd: isize, attribute: u32, data: *const std::ffi::c_void, size: u32) -> i32;
}

fn apply_style(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let symbol_fallback = fonts.families[&FontFamily::Monospace].clone();
    let pt_sans = include_bytes!("../assets/PTSans-Regular.ttf").to_vec();
    fonts.font_data.insert("pt-sans".to_owned(), std::sync::Arc::new(FontData::from_owned(pt_sans)));
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        if let Some(list) = fonts.families.get_mut(&family) {
            list.insert(0, "pt-sans".to_owned());
        }
    }
    fonts.families.get_mut(&FontFamily::Proportional).unwrap().extend(symbol_fallback);
    ctx.set_fonts(fonts);

    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = Color32::from_rgb(92, 78, 168);
    visuals.widgets.inactive.bg_fill = Color32::from_white_alpha(14);
    visuals.widgets.hovered.bg_fill = Color32::from_white_alpha(24);
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.extreme_bg_color = Color32::from_rgb(16, 12, 28);
    ctx.set_visuals(visuals);
}

#[cfg(test)]
mod ui_tests {
    use super::*;

    fn text_rect(shape: &egui::Shape, label: &str) -> Option<Rect> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => Some(Rect::from_min_size(text.pos, text.galley.size())),
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_rect(shape, label)),
            _ => None,
        }
    }

    fn test_launcher() -> (egui::Context, Launcher) {
        let ctx = egui::Context::default();
        apply_style(&ctx);
        let (tx, rx) = mpsc::channel();
        let launcher = Launcher {
            #[cfg(windows)]
            tray: None,
            root: std::env::temp_dir().join(format!("craft-ui-test-{}", std::process::id())),
            icons: HashMap::new(), uninstalled_icons: HashMap::new(), search: String::new(), filter: Filter::All,
            selected: Some(0), installed: HashMap::new(), remote: HashMap::new(), logs: HashMap::new(),
            recent: Vec::new(), favorites: Vec::new(), status: String::new(), error: None, busy: None, checking: false,
            logo: load_logo(&ctx), glass_hwnd: None, glass_maximized: None, patch_app: None,
            window_shape: None,
            uninstall_target: None, tx, rx,
            settings: settings::Settings::default(), settings_open: false, cancel: Arc::new(AtomicBool::new(false)),
            progress: None, retry: None, launcher_release: None, launcher_checking: false,
            launcher_status: String::new(), launcher_ready: None, usage: HashMap::new(), usage_selected: None,
            delete_portable_data: false,
        };
        (ctx, launcher)
    }

    #[test]
    fn clicking_star_does_not_open_app_popup() {
        let (ctx, mut launcher) = test_launcher();
        launcher.root = std::env::temp_dir().join(format!("craft-star-click-test-{}", std::process::id()));
        launcher.selected = None;
        let pos = pos2(52.0, 23.0);
        let mut render = |events| {
            let _ = ctx.run(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1220.0, 780.0))),
                events, ..Default::default()
            }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    launcher.tile(ui, Rect::from_min_size(pos2(0.0, 0.0), vec2(180.0, 164.0)), 0, 96.0);
                });
            });
        };
        for _ in 0..3 { render(Vec::new()); }
        render(vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default(),
        }]);
        render(vec![egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default(),
        }]);
        assert_eq!(launcher.favorites, vec!["photocraft"]);
        assert!(launcher.selected.is_none());
        std::fs::remove_file(launcher.root.join("favorites.json")).unwrap();
        std::fs::remove_dir(launcher.root).unwrap();
    }

    #[test]
    fn favorites_persist_and_filter_matches_search() {
        let (_, mut launcher) = test_launcher();
        launcher.root = std::env::temp_dir().join(format!("craft-favorites-test-{}", std::process::id()));
        launcher.toggle_favorite("photocraft");
        launcher.toggle_favorite("designcraft");
        launcher.favorites = update::load_favorites(&launcher.root);
        launcher.filter = Filter::Favorites;
        assert_eq!(launcher.visible(), vec![0, 6]);
        assert_eq!(launcher.nav_count(Filter::Favorites), 2);
        launcher.search = "design".into();
        assert_eq!(launcher.visible(), vec![6]);
        launcher.toggle_favorite("designcraft");
        assert!(launcher.visible().is_empty());
        assert_eq!(update::load_favorites(&launcher.root), vec!["photocraft"]);
        std::fs::remove_file(launcher.root.join("favorites.json")).unwrap();
        std::fs::remove_dir(launcher.root).unwrap();
    }

    #[test]
    fn failed_check_is_reported_instead_of_up_to_date() {
        let (_, mut launcher) = test_launcher();
        launcher.checking = true;
        launcher.tx.send(Event::CheckDone(vec!["PhotoCraft".into()])).unwrap();
        launcher.poll();
        assert!(!launcher.checking);
        assert!(launcher.error.as_deref().unwrap().contains("PhotoCraft"));
        assert!(!launcher.status.contains("up to date"));
    }

    #[test]
    fn failed_launch_keeps_app_details_and_shows_error() {
        let (_, mut launcher) = test_launcher();
        assert!(!launcher.open_index(0));
        assert_eq!(launcher.selected, Some(0));
        assert!(launcher.error.as_deref().unwrap().contains("Install"));
    }

    #[test]
    fn worker_failure_survives_idle_event() {
        let (_, mut launcher) = test_launcher();
        launcher.busy = Some("photocraft".into());
        launcher.tx.send(Event::Failed("Download failed".into())).unwrap();
        launcher.tx.send(Event::Idle { status: None }).unwrap();
        launcher.poll();
        assert!(launcher.busy.is_none());
        assert_eq!(launcher.error.as_deref(), Some("Download failed"));
    }

    #[test]
    fn install_button_click_works_without_update_check_results() {
        let (ctx, mut launcher) = test_launcher();
        let mut render = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1220.0, 780.0))),
                events, ..Default::default()
            };
            let mut requested = false;
            let output = ctx.run(input, |ctx| {
                requested = egui::Modal::new(Id::new("app-details-popup"))
                    .frame(egui::Frame::NONE).show(ctx, |ui| launcher.app_details(ui)).inner;
            });
            (requested, output)
        };
        for _ in 0..3 { render(Vec::new()); }
        let (_, output) = render(Vec::new());
        let button = output.shapes.iter().find_map(|shape| text_rect(&shape.shape, "Install"))
            .expect("Uninstalled app must show an Install button");
        let pos = button.center();
        render(vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default(),
        }]);
        let (requested, _) = render(vec![egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default(),
        }]);
        assert!(requested, "Clicking Install in the modal must request installation");
    }

    #[test]
    fn settings_update_button_centers_beside_version_and_status_at_common_scales() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let (ctx, mut launcher) = test_launcher();
            ctx.set_pixels_per_point(scale);
            launcher.settings_open = true;
            launcher.launcher_status = "Launcher is up to date".into();
            let mut render = || ctx.run(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1024.0, 720.0))), ..Default::default()
            }, |ctx| launcher.settings_panel(ctx));
            for _ in 0..3 { render(); }
            let output = render();
            let status = output.shapes.iter().find_map(|shape| text_rect(&shape.shape, "Launcher is up to date")).unwrap();
            let button = output.shapes.iter().find_map(|shape| text_rect(&shape.shape, "Check launcher updates")).unwrap();
            let title = output.shapes.iter().find_map(|shape| text_rect(&shape.shape, concat!("CraftLauncher v", env!("CARGO_PKG_VERSION")))).unwrap();
            assert!((title.union(status).center().y - button.center().y).abs() < 3.0, "Scale {scale}: button should center beside the text block");
            assert!(status.right() < button.left(), "Scale {scale}: text should not overlap the button");
        }
    }

    #[test]
    fn legacy_log_year_is_explicitly_estimated_without_changing_recorded_years() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        assert_eq!(log_date("10 Oct 14:26", today), ("10 Oct 2026".into(), true));
        assert_eq!(log_date("31 Dec 14:26", today), ("31 Dec 2025".into(), true));
        assert_eq!(log_date("10 Oct 2024", today), ("10 Oct 2024".into(), false));
        assert_eq!(log_date("Available", today), ("Available".into(), false));
    }
}
