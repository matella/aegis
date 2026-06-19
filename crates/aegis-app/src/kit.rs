//! Aegis UI kit — primitives visuelles du mockup (docs/aegis-mockup.html) : cartes, en-têtes,
//! libellés de section, badges d'état, boutons accent, jauges, séparateurs. Les vues s'appuient
//! dessus pour coller à la maquette au lieu d'empiler des widgets egui bruts.
use crate::theme::{ACCENT, BG, DIM, LINE, LINE2, PANEL, PANEL2, TEXT};
use egui::{Color32, CornerRadius, FontId, Margin, Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};

/// Carte : surface `panel`, bordure `line`, coins 9, marge interne — le conteneur de base.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(14, 13))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Carte avec en-tête `.hd` (gras 13px) puis corps.
pub fn card_titled<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    card(ui, |ui| {
        ui.label(RichText::new(title).size(13.0).strong().color(TEXT));
        ui.add_space(8.0);
        add(ui)
    })
}

/// Libellé de section autonome `.cap` (majuscules, sourd) — au-dessus d'une carte/liste.
pub fn cap(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).size(11.0).color(DIM));
}

/// Texte sourd (légende, méta).
pub fn dim(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).size(12.0).color(DIM))
}

/// Badge d'état (pill non cliquable) : texte mono coloré, bordure/fond teintés.
pub fn badge(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let font = FontId::monospace(11.0);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let pad = Vec2::new(8.0, 3.0);
    let (rect, resp) = ui.allocate_at_least(galley.size() + pad * 2.0, Sense::hover());
    let p = ui.painter();
    p.rect(
        rect,
        CornerRadius::same(10),
        color.gamma_multiply(0.10),
        Stroke::new(1.0, color.gamma_multiply(0.5)),
        StrokeKind::Inside,
    );
    p.galley(rect.center() - galley.size() / 2.0, galley, color);
    resp
}

/// Bouton primaire accent (`.btn.accent`) : fond accent, texte sombre.
pub fn btn_accent(ui: &mut Ui, text: &str) -> Response {
    let font = FontId::proportional(12.0);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, BG);
    let pad = Vec2::new(11.0, 6.0);
    let (rect, resp) = ui.allocate_at_least(galley.size() + pad * 2.0, Sense::click());
    let fill = if resp.hovered() { ACCENT.gamma_multiply(0.88) } else { ACCENT };
    let p = ui.painter();
    p.rect(rect, CornerRadius::same(7), fill, Stroke::NONE, StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, BG);
    resp
}

/// Pill cliquable à état (toggle) : remplie accent quand `on`, sinon contour.
pub fn toggle(ui: &mut Ui, text: &str, on: bool) -> Response {
    let font = FontId::proportional(11.0);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, TEXT);
    let pad = Vec2::new(11.0, 5.0);
    let (rect, resp) = ui.allocate_at_least(galley.size() + pad * 2.0, Sense::click());
    let (fill, fg, border) = if on {
        (ACCENT, BG, ACCENT)
    } else if resp.hovered() {
        (PANEL2, TEXT, ACCENT.gamma_multiply(0.6))
    } else {
        (Color32::TRANSPARENT, DIM, LINE2)
    };
    let p = ui.painter();
    p.rect(rect, CornerRadius::same(20), fill, Stroke::new(1.0, border), StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, fg);
    resp
}

/// Élément de navigation : icône (colonne fixe) + libellé, fond plein si sélectionné/survolé.
/// Garantit l'alignement quelle que soit la largeur du glyphe d'icône.
pub fn nav_item(ui: &mut Ui, icon: &str, label: &str, selected: bool) -> Response {
    let (rect, resp) = ui.allocate_at_least(Vec2::new(ui.available_width(), 34.0), Sense::click());
    let bg = if selected {
        PANEL2
    } else if resp.hovered() {
        PANEL2.gamma_multiply(0.6)
    } else {
        Color32::TRANSPARENT
    };
    let fg = if selected { TEXT } else { DIM };
    let p = ui.painter();
    if bg != Color32::TRANSPARENT {
        p.rect_filled(rect, CornerRadius::same(7), bg);
    }
    let icon_g = p.layout_no_wrap(icon.to_owned(), FontId::proportional(14.0), fg);
    p.galley(egui::pos2(rect.left() + 11.0, rect.center().y - icon_g.size().y / 2.0), icon_g, fg);
    let label_g = p.layout_no_wrap(label.to_owned(), FontId::proportional(13.0), fg);
    p.galley(egui::pos2(rect.left() + 38.0, rect.center().y - label_g.size().y / 2.0), label_g, fg);
    resp
}

/// Jauge fine pleine largeur (VRAM…). `frac` ∈ [0,1].
pub fn gauge(ui: &mut Ui, frac: f32, fill: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 7.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(0x22, 0x28, 0x31));
    let mut fr = rect;
    fr.set_width(rect.width() * frac.clamp(0.0, 1.0));
    p.rect_filled(fr, CornerRadius::same(4), fill);
}

/// Séparateur fin pleine largeur (entre lignes d'une liste).
pub fn hairline(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, LINE));
}

/// Tuile de statistique : libellé « cap » + grande valeur en chasse fixe.
pub fn stat(ui: &mut Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        cap(ui, label);
        ui.add_space(2.0);
        ui.label(RichText::new(value).font(FontId::monospace(15.0)).color(TEXT));
    });
}

/// Pastille d'état `.led` : cercle plein + léger halo. Remplace le glyphe « ● » (absent de la police
/// egui → rendu en tofu). Aligné verticalement sur la hauteur du texte courant.
pub fn led(ui: &mut Ui, color: Color32) {
    let h = ui.text_style_height(&egui::TextStyle::Body);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(12.0, h), Sense::hover());
    let c = rect.center();
    let p = ui.painter();
    p.circle_filled(c, 6.0, color.gamma_multiply(0.18)); // halo
    p.circle_filled(c, 4.0, color);
}

/// Nombre en chasse fixe — couleur libre.
pub fn mono(text: impl Into<String>, color: Color32) -> RichText {
    RichText::new(text).font(FontId::monospace(11.0)).color(color)
}
