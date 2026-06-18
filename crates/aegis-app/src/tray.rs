//! Tray + menu (Show/Quit). Show appelle `WindowCtl::show()` (Win32 direct sur Windows → fiable même
//! fenêtre masquée). Quit = `std::process::exit(0)` (bulletproof). Tolérant : échec de build = no-op.
use crate::window::WindowCtl;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub struct Tray {
    _icon: Option<TrayIcon>,
}

impl Tray {
    pub fn new(win: WindowCtl) -> Self {
        Self {
            _icon: build(win).ok(),
        }
    }
}

fn build(win: WindowCtl) -> Result<TrayIcon, Box<dyn std::error::Error>> {
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

    let w = win.clone();
    MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
        if ev.id == show_id {
            w.show();
        } else if ev.id == quit_id {
            std::process::exit(0);
        }
    }));

    let w2 = win;
    TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
        if let TrayIconEvent::Click { .. } = ev {
            w2.show();
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
