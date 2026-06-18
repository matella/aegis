pub mod chat;
pub mod dashboard;
pub mod settings;
pub mod uploader;

// Couleurs sémantiques partagées par les vues — tokens du mockup.
pub use crate::theme::{DIM, GREEN, RED};

/// Titre de page `.ph` (gras 15px) — egui::heading est trop gros pour la maquette.
pub fn page_title(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title).size(15.0).strong().color(crate::theme::TEXT));
}

/// En-tête de vue : titre `.ph` + légende mono sourde à droite (`.lat`), puis un peu d'air.
pub fn header(ui: &mut egui::Ui, title: &str, caption: &str) {
    ui.horizontal(|ui| {
        page_title(ui, title);
        if !caption.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(crate::kit::mono(caption, DIM));
            });
        }
    });
    ui.add_space(12.0);
}
