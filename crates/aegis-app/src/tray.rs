//! Icône de tray + menu (Show/Quit). L'app reste en arrière-plan ; réduire/fermer la masquent ici.
//! Tolérant : si la création échoue (plateforme/permissions), on dégrade sans crash.
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub enum TrayAction {
    Show,
    Quit,
}

pub struct Tray {
    _icon: Option<TrayIcon>,
    show_id: Option<MenuId>,
    quit_id: Option<MenuId>,
}

impl Tray {
    pub fn new() -> Self {
        match Self::build() {
            Ok((icon, show, quit)) => Self {
                _icon: Some(icon),
                show_id: Some(show),
                quit_id: Some(quit),
            },
            Err(_) => Self {
                _icon: None,
                show_id: None,
                quit_id: None,
            },
        }
    }

    fn build() -> Result<(TrayIcon, MenuId, MenuId), Box<dyn std::error::Error>> {
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
        Ok((tray, show.id().clone(), quit.id().clone()))
    }

    /// À appeler chaque frame : draine les events tray/menu et renvoie l'action éventuelle.
    pub fn poll(&self) -> Option<TrayAction> {
        // Clic sur l'icône → afficher.
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { .. } = ev {
                return Some(TrayAction::Show);
            }
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if Some(&ev.id) == self.show_id.as_ref() {
                return Some(TrayAction::Show);
            }
            if Some(&ev.id) == self.quit_id.as_ref() {
                return Some(TrayAction::Quit);
            }
        }
        None
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
