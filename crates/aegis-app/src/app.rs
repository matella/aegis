//! AegisApp : état partagé + boucle egui. Logique métier dans aegis-core/client ; ici, du câblage.
use crate::{poll, tray, views};
use aegis_core::{Config, GpuState, NowPlaying, ServiceHealth};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Données affichées par le Dashboard (alimentées par le poller).
#[derive(Default)]
pub struct DashboardData {
    pub health: Vec<ServiceHealth>,
    pub gpu: Option<GpuState>,
    pub gpu_error: Option<String>,
    pub now: NowPlaying,
}

#[derive(PartialEq, Clone, Copy)]
pub enum Panel {
    Dashboard,
    Uploader,
    Settings,
}

pub struct AegisApp {
    pub rt: tokio::runtime::Handle,
    pub cfg: Arc<Mutex<Config>>, // partagé avec le poller (relu à chaque tick)
    pub dash: Arc<Mutex<DashboardData>>,
    pub up_state: aegis_uploader::SharedState,
    pub up_tx: aegis_uploader::EventSender,
    pub watchers: Vec<aegis_uploader::watcher::WatcherHandle>,
    pub panel: Panel,
    pub tray: tray::Tray,
    pub really_quit: bool,
    // tampons d'édition (Settings)
    pub e_jarvis: String,
    pub e_jarvis_token: String,
    pub e_storm: String,
    pub e_orpheus: String,
    pub e_upload_token: String,
    pub save_msg: Option<(String, bool)>,
    // uploader : ajout de dossier
    pub new_dir: String,
}

impl AegisApp {
    pub fn new(cc: &eframe::CreationContext<'_>, rt: tokio::runtime::Handle, cfg: Config) -> Self {
        let ctx = cc.egui_ctx.clone();

        // État uploader (serveur/token = config Aegis ; dossiers + uploaded depuis le disque).
        let dirs = aegis_uploader::settings::load_dirs();
        let uploaded = aegis_uploader::settings::load_uploaded();
        let up_state = Arc::new(Mutex::new(aegis_uploader::AppState::new(
            cfg.storm_codex_url.clone(),
            cfg.upload_token.clone(),
            dirs.clone(),
            uploaded,
        )));
        {
            let ctx2 = ctx.clone();
            up_state.lock().unwrap().repaint = Some(Arc::new(move || ctx2.request_repaint()));
        }
        let (tx, rx) = aegis_uploader::event_channel();
        // La vue lit l'état directement → on draine les events pour éviter l'accumulation.
        rt.spawn(async move {
            let mut rx = rx;
            while rx.recv().await.is_some() {}
        });
        let watchers = boot_uploader(&rt, &up_state, &tx, &dirs);

        // Dashboard + poller.
        let cfg_arc = Arc::new(Mutex::new(cfg.clone()));
        let dash = Arc::new(Mutex::new(DashboardData::default()));
        poll::spawn_poller(rt.clone(), cfg_arc.clone(), dash.clone(), ctx.clone());

        Self {
            rt,
            cfg: cfg_arc,
            dash,
            up_state,
            up_tx: tx,
            watchers,
            panel: Panel::Dashboard,
            tray: tray::Tray::new(&ctx),
            really_quit: false,
            e_jarvis: cfg.jarvis_url,
            e_jarvis_token: cfg.jarvis_token.unwrap_or_default(),
            e_storm: cfg.storm_codex_url,
            e_orpheus: cfg.orpheus_url,
            e_upload_token: cfg.upload_token.unwrap_or_default(),
            save_msg: None,
            new_dir: String::new(),
        }
    }

    /// Applique les réglages édités : persiste, met à jour la config partagée (poller) et l'état
    /// uploader (serveur/token). Les watchers se (re)démarrent via la vue Uploader.
    pub fn apply_settings(&mut self) {
        let cfg = Config {
            jarvis_url: self.e_jarvis.trim().to_string(),
            jarvis_token: opt(&self.e_jarvis_token),
            storm_codex_url: self.e_storm.trim().to_string(),
            orpheus_url: self.e_orpheus.trim().to_string(),
            upload_token: opt(&self.e_upload_token),
        };
        let saved = cfg.save();
        {
            let mut s = self.up_state.lock().unwrap();
            s.server_url = cfg.storm_codex_url.clone();
            s.auth_token = cfg.upload_token.clone();
            s.server_connected = false;
        }
        *self.cfg.lock().unwrap() = cfg;
        self.save_msg = Some(match saved {
            Ok(()) => ("Saved.".into(), true),
            Err(e) => (format!("Save failed: {e}"), false),
        });
    }

    /// (Re)démarre les watchers sur les dossiers persistés (après ajout/suppression).
    pub fn restart_watchers(&mut self) {
        for h in self.watchers.drain(..) {
            h.stop();
        }
        let dirs = { self.up_state.lock().unwrap().replay_dirs.clone() };
        self.watchers = boot_uploader(&self.rt, &self.up_state, &self.up_tx, &dirs);
    }
}

fn opt(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Démarre le check de connectivité + un watcher par dossier + un scan initial. Renvoie les handles
/// (à garder vivants). Mirroir de client-rs/main.rs.
fn boot_uploader(
    rt: &tokio::runtime::Handle,
    state: &aegis_uploader::SharedState,
    tx: &aegis_uploader::EventSender,
    dirs: &[String],
) -> Vec<aegis_uploader::watcher::WatcherHandle> {
    let server_empty = state.lock().unwrap().server_url.is_empty();
    if !server_empty {
        aegis_uploader::uploader::start_connectivity_check(state.clone(), tx.clone(), rt.clone());
    }
    let mut handles = Vec::new();
    let mut scan = Vec::new();
    for dir in dirs {
        if PathBuf::from(dir).is_dir() {
            if let Ok(h) = aegis_uploader::watcher::start_watcher(dir, state.clone(), tx.clone(), rt.clone()) {
                handles.push(h);
                scan.push(dir.clone());
            }
        }
    }
    if !scan.is_empty() {
        let (s, t) = (state.clone(), tx.clone());
        rt.spawn(async move {
            for d in scan {
                aegis_uploader::uploader::scan_and_upload(&PathBuf::from(d), &s, &t).await;
            }
        });
    }
    if !handles.is_empty() {
        state.lock().unwrap().watcher_status = aegis_uploader::WatcherStatus::Watching;
    }
    handles
}

impl eframe::App for AegisApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Événements du tray.
        if let Some(action) = self.tray.poll() {
            match action {
                tray::TrayAction::Show => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                tray::TrayAction::Quit => {
                    self.really_quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        // Fermeture → masquer dans le tray (sauf Quit explicite) ; l'app continue de tourner.
        if !self.really_quit && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        // Réduction → tray.
        if ctx.input(|i| i.viewport().minimized == Some(true)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        egui::SidePanel::left("nav").exact_width(168.0).show(ctx, |ui| {
            ui.add_space(8.0);
            ui.heading("Aegis");
            ui.add_space(10.0);
            let mut nav = |ui: &mut egui::Ui, p: Panel, label: &str| {
                if ui.selectable_label(self.panel == p, label).clicked() {
                    self.panel = p;
                }
            };
            nav(ui, Panel::Dashboard, "▦  Dashboard");
            nav(ui, Panel::Uploader, "⇪  Uploader");
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
            nav(ui, Panel::Settings, "⚙  Settings");
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.panel {
            Panel::Dashboard => views::dashboard::ui(self, ui),
            Panel::Uploader => views::uploader::ui(self, ui),
            Panel::Settings => views::settings::ui(self, ui),
        });
    }
}
