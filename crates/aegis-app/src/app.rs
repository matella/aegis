//! AegisApp : état partagé + boucle egui. Logique métier dans aegis-core/client ; ici, du câblage.
use crate::{audio, chat, poll, theme, tray, views, window};
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
    Jarvis,
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
    pub chat: Arc<Mutex<chat::ChatState>>,
    pub chat_tx: tokio::sync::mpsc::UnboundedSender<chat::Outbound>,
    pub chat_input: String,
    pub listening: bool,
    pub listen_handle: Option<audio::ListenHandle>,
    pub win: window::WindowCtl,
    #[allow(dead_code)] // gardé vivant : drop = l'icône tray disparaît
    pub tray: tray::Tray,
    // tampons d'édition (Settings)
    pub e_jarvis: String,
    pub e_jarvis_token: String,
    pub e_storm: String,
    pub e_orpheus: String,
    pub e_upload_token: String,
    pub e_mic: String,
    pub e_output: String,
    pub e_wake: String,
    pub e_thinking: String,
    pub mic_devices: Vec<String>,
    pub out_devices: Vec<String>,
    pub save_msg: Option<(String, bool)>,
    // uploader : ajout de dossier
    pub new_dir: String,
}

impl AegisApp {
    pub fn new(cc: &eframe::CreationContext<'_>, rt: tokio::runtime::Handle, cfg: Config) -> Self {
        let ctx = cc.egui_ctx.clone();
        crate::theme::apply(&ctx); // thème dark Aegis
        egui_extras::install_image_loaders(&ctx); // pochette now-playing (URL → texture)

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

        let win = window::WindowCtl::new(ctx.clone());

        // Chat Jarvis (WS /ws, reconnexion auto) + lecteur audio (TTS + chimes) sur la sortie choisie.
        let player = audio::Player::new(cfg.output_device.clone());
        let chat = Arc::new(Mutex::new(chat::ChatState::default()));
        let chat_tx = chat::spawn(
            &rt,
            cfg.jarvis_url.clone(),
            cfg_arc.clone(),
            chat.clone(),
            ctx.clone(),
            player, // déplacé : le thread de lecture TTS vit tant que le client chat existe
        );

        Self {
            rt,
            cfg: cfg_arc,
            dash,
            up_state,
            up_tx: tx,
            watchers,
            panel: Panel::Dashboard,
            chat,
            chat_tx,
            chat_input: String::new(),
            listening: false,
            listen_handle: None,
            tray: tray::Tray::new(win.clone()),
            win,
            e_jarvis: cfg.jarvis_url,
            e_jarvis_token: cfg.jarvis_token.unwrap_or_default(),
            e_storm: cfg.storm_codex_url,
            e_orpheus: cfg.orpheus_url,
            e_upload_token: cfg.upload_token.unwrap_or_default(),
            e_mic: cfg.mic_device.clone().unwrap_or_default(),
            e_output: cfg.output_device.clone().unwrap_or_default(),
            e_wake: cfg.wake_reply.clone(),
            e_thinking: cfg.thinking_reply.clone(),
            mic_devices: audio::input_devices(),
            out_devices: audio::output_devices(),
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
            mic_device: opt(&self.e_mic),
            output_device: opt(&self.e_output),
            wake_reply: self.e_wake.trim().to_string(),
            thinking_reply: self.e_thinking.trim().to_string(),
        };
        let saved = cfg.save();
        {
            let mut s = self.up_state.lock().unwrap();
            s.server_url = cfg.storm_codex_url.clone();
            s.auth_token = cfg.upload_token.clone();
            s.server_connected = false;
        }
        *self.cfg.lock().unwrap() = cfg;
        // Micro : si on écoute, redémarre la capture avec le périphérique choisi (effet immédiat).
        // Sortie audio : appliquée au prochain lancement (le lecteur est créé au démarrage).
        if self.listening {
            let mic = self.cfg.lock().unwrap().mic_device.clone();
            self.listen_handle = audio::start_listening(self.chat_tx.clone(), mic);
        }
        self.save_msg = Some(match saved {
            Ok(()) => ("Saved. (sortie audio : relance l'app pour l'appliquer)".into(), true),
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

/// Extrait l'hôte (sans schéma ni port) d'une URL — pour l'étiquette « box · <host> » de la sidebar.
fn box_host(url: &str) -> Option<String> {
    let after = url.split("://").nth(1).unwrap_or(url);
    let host = after.split(['/', ':']).next().unwrap_or("").trim();
    (!host.is_empty()).then(|| host.to_string())
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
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Capte le handle natif (Windows) au 1er frame → pilotage show/hide fiable depuis le tray.
        #[cfg(windows)]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = frame.window_handle() {
                if let RawWindowHandle::Win32(w) = h.as_raw() {
                    self.win.set_hwnd(w.hwnd.get());
                }
            }
        }
        #[cfg(not(windows))]
        let _ = frame;

        // Fermeture ET réduction → masquer dans le tray (Win32 direct) ; l'app continue de tourner.
        // Quit se fait via le menu tray (process::exit), pas par le bouton de fermeture.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.win.hide();
        }
        if ctx.input(|i| i.viewport().minimized == Some(true)) {
            self.win.hide();
        }

        egui::SidePanel::left("nav")
            .exact_width(172.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::SIDE)
                    .stroke(egui::Stroke::new(1.0, theme::LINE))
                    .inner_margin(egui::Margin::symmetric(10, 12)),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;

                // Marque + box (hôte dérivé de l'URL storm-codex), façon « box · IP » du mockup.
                ui.horizontal(|ui| {
                    ui.add_space(1.0);
                    ui.label(egui::RichText::new("Aegis").size(16.0).strong().color(theme::ACCENT));
                });
                if let Some(host) = box_host(&self.cfg.lock().unwrap().storm_codex_url) {
                    ui.label(egui::RichText::new(format!("box · {host}")).size(10.0).color(theme::DIM).monospace());
                }
                ui.add_space(10.0);
                crate::kit::hairline(ui);
                ui.add_space(8.0);

                let mut nav = |ui: &mut egui::Ui, p: Panel, icon: &str, label: &str| {
                    if crate::kit::nav_item(ui, icon, label, self.panel == p).clicked() {
                        self.panel = p;
                    }
                };
                // Icônes = glyphes emoji (seuls rendus par la police egui ; ▦/⇪/✦ → tofu).
                nav(ui, Panel::Dashboard, "📊", "Dashboard");
                nav(ui, Panel::Uploader, "📤", "Uploader");
                nav(ui, Panel::Jarvis, "✨", "Jarvis");
                ui.add_space(8.0);
                crate::kit::hairline(ui);
                ui.add_space(8.0);
                nav(ui, Panel::Settings, "⚙", "Settings");

                // Pied : état de connexion Jarvis, séparé par un hairline.
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    let connected = self.chat.lock().unwrap().connected;
                    let (c, t) = if connected {
                        (theme::GREEN, "jarvis online")
                    } else {
                        (theme::RED, "jarvis offline")
                    };
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        crate::kit::led(ui, c);
                        ui.label(egui::RichText::new(t).size(11.0).color(theme::DIM));
                    });
                    ui.add_space(8.0);
                    crate::kit::hairline(ui);
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BG).inner_margin(egui::Margin::same(18)))
            .show(ctx, |ui| match self.panel {
                Panel::Dashboard => views::dashboard::ui(self, ui),
                Panel::Jarvis => views::chat::ui(self, ui),
                Panel::Uploader => views::uploader::ui(self, ui),
                Panel::Settings => views::settings::ui(self, ui),
            });
    }
}
