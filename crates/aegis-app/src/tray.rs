//! Icône de tray + menu (Show/Quit). Point clé : fenêtre masquée, egui n'appelle plus `update()`.
//! On installe donc des HANDLERS d'events (menu + icône) qui empilent une action ET réveillent egui
//! (`request_repaint`) → `update()` retourne tourner et traite l'action. Tolérant : échec = no-op.
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(Clone, Copy)]
pub enum TrayAction {
    Show,
    Quit,
}

type Queue = Arc<Mutex<VecDeque<TrayAction>>>;

pub struct Tray {
    _icon: Option<TrayIcon>,
    queue: Queue,
}

impl Tray {
    pub fn new(ctx: &egui::Context) -> Self {
        let queue: Queue = Arc::new(Mutex::new(VecDeque::new()));
        let icon = Self::build(ctx, queue.clone()).ok();
        Self { _icon: icon, queue }
    }

    fn build(ctx: &egui::Context, queue: Queue) -> Result<TrayIcon, Box<dyn std::error::Error>> {
        let menu = Menu::new();
        let show = MenuItem::new("Show Aegis", true, None);
        let quit = MenuItem::new("Quit", true, None);
        menu.append(&show)?;
        menu.append(&quit)?;
        let tray = TrayIconBuilder::new()
            .with_tooltip("Aegis")
            .with_menu(Box::new(menu))
            .with_icon(make_icon())
            .build()?;

        let show_id = show.id().clone();
        let quit_id = quit.id().clone();

        // Menu : empile l'action + réveille egui.
        let (mq, mctx) = (queue.clone(), ctx.clone());
        MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
            let action = if ev.id == show_id {
                Some(TrayAction::Show)
            } else if ev.id == quit_id {
                Some(TrayAction::Quit)
            } else {
                None
            };
            if let Some(a) = action {
                mq.lock().unwrap().push_back(a);
                mctx.request_repaint();
            }
        }));

        // Clic gauche sur l'icône → afficher.
        let (tq, tctx) = (queue, ctx.clone());
        TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
            if let TrayIconEvent::Click { .. } = ev {
                tq.lock().unwrap().push_back(TrayAction::Show);
                tctx.request_repaint();
            }
        }));

        Ok(tray)
    }

    /// À appeler chaque frame : renvoie la prochaine action tray en attente.
    pub fn poll(&self) -> Option<TrayAction> {
        self.queue.lock().unwrap().pop_front()
    }
}

fn make_icon() -> Icon {
    let (w, h) = (16u32, 16u32);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..(w * h) {
        rgba.extend_from_slice(&[0x7e, 0x97, 0xff, 0xff]); // accent plein
    }
    Icon::from_rgba(rgba, w, h).expect("tray icon")
}
