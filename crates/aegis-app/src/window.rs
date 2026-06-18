//! Pilotage show/hide de la fenêtre — fiable même « masquée dans le tray ». Sur Windows on appelle
//! directement l'API Win32 (`ShowWindow`) via le HWND, ce qui NE dépend pas de la boucle eframe (qui
//! ne tourne plus quand la fenêtre est cachée — la cause de tous les ratés tray). Ailleurs (dev
//! macOS), on retombe sur les viewport commands egui.
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct WindowCtl {
    ctx: egui::Context,
    #[allow(dead_code)] // utilisé uniquement sur Windows (pilotage Win32)
    hwnd: Arc<Mutex<Option<isize>>>,
}

impl WindowCtl {
    pub fn new(ctx: egui::Context) -> Self {
        Self {
            ctx,
            hwnd: Arc::new(Mutex::new(None)),
        }
    }

    /// Enregistre le handle natif de la fenêtre (capté au 1er frame depuis `eframe::Frame`).
    #[allow(dead_code)] // appelé uniquement sur Windows
    pub fn set_hwnd(&self, h: isize) {
        *self.hwnd.lock().unwrap() = Some(h);
    }

    pub fn hide(&self) {
        #[cfg(windows)]
        self.hide_win();
        #[cfg(not(windows))]
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    pub fn show(&self) {
        #[cfg(windows)]
        self.show_win();
        #[cfg(not(windows))]
        {
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.ctx.request_repaint();
    }

    #[cfg(windows)]
    fn hwnd(&self) -> Option<windows::Win32::Foundation::HWND> {
        self.hwnd
            .lock()
            .unwrap()
            .map(|h| windows::Win32::Foundation::HWND(h as *mut core::ffi::c_void))
    }

    #[cfg(windows)]
    fn hide_win(&self) {
        use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
        if let Some(w) = self.hwnd() {
            unsafe {
                let _ = ShowWindow(w, SW_HIDE);
            }
        }
    }

    #[cfg(windows)]
    fn show_win(&self) {
        use windows::Win32::UI::WindowsAndMessaging::{
            SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
        };
        if let Some(w) = self.hwnd() {
            unsafe {
                let _ = ShowWindow(w, SW_SHOW);
                let _ = ShowWindow(w, SW_RESTORE);
                let _ = SetForegroundWindow(w);
            }
        }
    }
}
