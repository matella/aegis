//! Vue Settings : endpoints + tokens (édités puis persistés, rien de gravé). Le poller relit la
//! config au tick suivant ; l'état uploader (serveur/token) est mis à jour à la sauvegarde.
use crate::app::AegisApp;
use crate::kit;
use crate::theme::DIM;
use crate::views::{GREEN, RED};

pub fn ui(app: &mut AegisApp, ui: &mut egui::Ui) {
    super::header(ui, "Connections", "");

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // Largeur contrainte façon mockup (max ~520).
        ui.set_max_width(520.0);

        field(ui, "Jarvis gateway", |ui| {
            ui.add(egui::TextEdit::singleline(&mut app.e_jarvis).desired_width(f32::INFINITY));
        });
        field(ui, "Jarvis token", |ui| {
            ui.add(egui::TextEdit::singleline(&mut app.e_jarvis_token).password(true).desired_width(f32::INFINITY));
        });
        field(ui, "storm-codex", |ui| {
            ui.add(egui::TextEdit::singleline(&mut app.e_storm).desired_width(f32::INFINITY));
        });
        field(ui, "Orpheus", |ui| {
            ui.add(egui::TextEdit::singleline(&mut app.e_orpheus).desired_width(f32::INFINITY));
        });
        field(ui, "Upload token (storm-codex)", |ui| {
            ui.add(egui::TextEdit::singleline(&mut app.e_upload_token).password(true).desired_width(f32::INFINITY));
        });

        ui.add_space(18.0);
        kit::cap(ui, "Voice");
        ui.add_space(8.0);

        let mics = app.mic_devices.clone();
        let outs = app.out_devices.clone();
        field(ui, "Microphone", |ui| {
            egui::ComboBox::from_id_salt("mic")
                .width(ui.available_width())
                .selected_text(if app.e_mic.is_empty() { "System default".to_string() } else { app.e_mic.clone() })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.e_mic, String::new(), "System default");
                    for d in &mics {
                        ui.selectable_value(&mut app.e_mic, d.clone(), d);
                    }
                });
        });
        field(ui, "Output (sound)", |ui| {
            egui::ComboBox::from_id_salt("out")
                .width(ui.available_width())
                .selected_text(if app.e_output.is_empty() { "System default".to_string() } else { app.e_output.clone() })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.e_output, String::new(), "System default");
                    for d in &outs {
                        ui.selectable_value(&mut app.e_output, d.clone(), d);
                    }
                });
        });
        field(ui, "Wake reply", |ui| {
            ui.add(egui::TextEdit::singleline(&mut app.e_wake).desired_width(f32::INFINITY));
        });
        field(ui, "Thinking reply", |ui| {
            ui.add(egui::TextEdit::singleline(&mut app.e_thinking).desired_width(f32::INFINITY));
        });

        ui.add_space(16.0);
        ui.horizontal(|ui| {
            if kit::btn_accent(ui, "Save & test").clicked() {
                app.apply_settings();
            }
            ui.add_space(6.0);
            if let Some((msg, ok)) = &app.save_msg {
                kit::badge(ui, msg, if *ok { GREEN } else { RED });
            }
        });
        ui.add_space(14.0);
        ui.label(
            egui::RichText::new(
                "Wake reply = dit quand « hey jarvis » est entendu ; Thinking reply = quand tu as fini de \
                 parler. Vide = juste un bip. Endpoints + tokens persistés en local — rien n'est gravé dans \
                 le binaire. (Sortie audio : relance l'app pour l'appliquer.)",
            )
            .size(12.0)
            .color(DIM),
        );
    });
}

/// Champ : libellé sourd au-dessus + widget pleine largeur (style mockup).
fn field(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(10.0);
    ui.label(egui::RichText::new(label).size(11.0).color(DIM));
    ui.add_space(4.0);
    add(ui);
}
