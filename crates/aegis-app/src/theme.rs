//! Thème dark Aegis (palette du mockup) appliqué globalement à egui : fond profond, accent indigo,
//! cartes, sélection. Toutes les vues en héritent.
use egui::{Color32, Stroke};

// Tokens du mockup (docs/aegis-mockup.html).
pub const ACCENT: Color32 = Color32::from_rgb(0x7e, 0x97, 0xff);
pub const BG: Color32 = Color32::from_rgb(0x0d, 0x0f, 0x12);
/// Surface très sombre : sidebar + titlebar du mockup (#0a0c0f).
pub const SIDE: Color32 = Color32::from_rgb(0x0a, 0x0c, 0x0f);
pub const PANEL: Color32 = Color32::from_rgb(0x15, 0x18, 0x1d);
pub const PANEL2: Color32 = Color32::from_rgb(0x1b, 0x1f, 0x25);
pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe9, 0xee);
pub const DIM: Color32 = Color32::from_rgb(0x8b, 0x94, 0xa2);
pub const LINE: Color32 = Color32::from_rgb(0x27, 0x2c, 0x34);
pub const LINE2: Color32 = Color32::from_rgb(0x33, 0x3a, 0x44);
pub const GREEN: Color32 = Color32::from_rgb(0x3f, 0xb9, 0x50);
pub const RED: Color32 = Color32::from_rgb(0xf8, 0x51, 0x49);
/// Amber : actions destructives « confirmation requise » du chat (mockup #2/#4).
#[allow(dead_code)]
pub const AMBER: Color32 = Color32::from_rgb(0xd2, 0x99, 0x22);

pub fn apply(ctx: &egui::Context) {
    let mut style = egui::Style::default();
    let mut v = egui::Visuals::dark();

    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.window_fill = BG;
    v.panel_fill = BG;
    v.faint_bg_color = PANEL;
    v.extreme_bg_color = Color32::from_rgb(0x0a, 0x0c, 0x0f); // fond des champs de saisie
    v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, ACCENT);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, DIM);

    w.inactive.bg_fill = PANEL2;
    w.inactive.weak_bg_fill = PANEL2;
    w.inactive.bg_stroke = Stroke::new(1.0, LINE);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);

    w.hovered.bg_fill = Color32::from_rgb(0x23, 0x29, 0x33);
    w.hovered.weak_bg_fill = Color32::from_rgb(0x23, 0x29, 0x33);
    w.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    w.hovered.fg_stroke = Stroke::new(1.0, TEXT);

    w.active.bg_fill = ACCENT;
    w.active.weak_bg_fill = ACCENT;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.0, BG);

    // Coins arrondis cohérents (cartes, boutons, champs).
    for s in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        s.corner_radius = egui::CornerRadius::same(6);
    }

    style.visuals = v;
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    ctx.set_style(style);
}

/// Icône d'app/fenêtre 32×32 : carré accent avec coins adoucis (mieux que le défaut).
pub fn app_icon() -> egui::IconData {
    let (w, h) = (32usize, 32usize);
    let mut rgba = vec![0u8; w * h * 4];
    let bg = [0x0d, 0x0f, 0x12, 0xff];
    let ac = [0x7e, 0x97, 0xff, 0xff];
    for y in 0..h {
        for x in 0..w {
            // bloc accent centré avec marge → look "carré arrondi"
            let inset = (x >= 4 && x < w - 4 && y >= 4 && y < h - 4) as usize;
            let corner = ((x < 7 || x >= w - 7) && (y < 7 || y >= h - 7)) as usize;
            let px = if inset == 1 && corner == 0 { ac } else { bg };
            let i = (y * w + x) * 4;
            rgba[i..i + 4].copy_from_slice(&px);
        }
    }
    egui::IconData { rgba, width: w as u32, height: h as u32 }
}
