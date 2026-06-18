//! Tray + menu (Show/Quit). Le piège eframe+Windows : fenêtre masquée, `update()` ne tourne plus →
//! traiter l'action DANS update() ne marche pas. La solution : le HANDLER d'event (qui tourne sur le
//! thread de la boucle winit, même fenêtre cachée) envoie LUI-MÊME la commande de fenêtre via le
//! `egui::Context` (`send_viewport_cmd`) + `request_repaint()` pour réveiller eframe. Tolérant.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub struct Tray {
    _icon: Option<TrayIcon>,
}

impl Tray {
    /// `really_quit` : partagé avec l'app ; mis à true quand l'utilisateur choisit Quit, pour que le
    /// handler de fermeture de l'app NE masque PAS (laisse fermer).
    pub fn new(ctx: &egui::Context, really_quit: Arc<AtomicBool>) -> Self {
        Self {
            _icon: build(ctx, really_quit).ok(),
        }
    }
}

fn show(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}

fn build(ctx: &egui::Context, really_quit: Arc<AtomicBool>) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let menu = Menu::new();
    let show_item = MenuItem::new("Show Aegis", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    menu.append(&show_item)?;
    menu.append(&quit_item)?;
    let tray = TrayIconBuilder::new()
        .with_tooltip("Aegis")
        .with_menu(Box::new(menu))
        .with_icon(make_icon())
        .build()?;

    let show_id = show_item.id().clone();
    let quit_id = quit_item.id().clone();

    // Menu : agit directement sur la fenêtre depuis le handler (fonctionne fenêtre masquée).
    let mctx = ctx.clone();
    MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
        if ev.id == show_id {
            show(&mctx);
        } else if ev.id == quit_id {
            really_quit.store(true, Ordering::SeqCst);
            mctx.send_viewport_cmd(egui::ViewportCommand::Close);
            mctx.request_repaint();
        }
    }));

    // Clic gauche sur l'icône → afficher.
    let tctx = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
        if let TrayIconEvent::Click { .. } = ev {
            show(&tctx);
        }
    }));

    Ok(tray)
}

fn make_icon() -> Icon {
    let (w, h) = (16u32, 16u32);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..(w * h) {
        rgba.extend_from_slice(&[0x7e, 0x97, 0xff, 0xff]); // accent plein
    }
    Icon::from_rgba(rgba, w, h).expect("tray icon")
}
