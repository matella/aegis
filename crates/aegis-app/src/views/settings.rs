//! Vue Settings : endpoints + tokens (édités puis persistés, rien de gravé). Le poller relit la
//! config au tick suivant ; l'état uploader (serveur/token) est mis à jour à la sauvegarde.
use crate::app::AegisApp;
use crate::views::{GREEN, RED};

pub fn ui(app: &mut AegisApp, ui: &mut egui::Ui) {
    ui.heading("Settings");
    ui.add_space(8.0);

    egui::Grid::new("cfg").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Jarvis gateway");
        ui.text_edit_singleline(&mut app.e_jarvis);
        ui.end_row();
        ui.label("Jarvis token");
        ui.add(egui::TextEdit::singleline(&mut app.e_jarvis_token).password(true));
        ui.end_row();
        ui.label("storm-codex");
        ui.text_edit_singleline(&mut app.e_storm);
        ui.end_row();
        ui.label("Orpheus");
        ui.text_edit_singleline(&mut app.e_orpheus);
        ui.end_row();
        ui.label("Upload token");
        ui.add(egui::TextEdit::singleline(&mut app.e_upload_token).password(true));
        ui.end_row();
    });

    ui.add_space(12.0);
    ui.label(egui::RichText::new("VOICE").small().weak());
    let mics = app.mic_devices.clone();
    let outs = app.out_devices.clone();
    egui::Grid::new("voice").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Microphone");
        egui::ComboBox::from_id_salt("mic")
            .selected_text(if app.e_mic.is_empty() { "System default".to_string() } else { app.e_mic.clone() })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.e_mic, String::new(), "System default");
                for d in &mics {
                    ui.selectable_value(&mut app.e_mic, d.clone(), d);
                }
            });
        ui.end_row();
        ui.label("Output (sound)");
        egui::ComboBox::from_id_salt("out")
            .selected_text(if app.e_output.is_empty() { "System default".to_string() } else { app.e_output.clone() })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.e_output, String::new(), "System default");
                for d in &outs {
                    ui.selectable_value(&mut app.e_output, d.clone(), d);
                }
            });
        ui.end_row();
        ui.label("Wake reply");
        ui.text_edit_singleline(&mut app.e_wake);
        ui.end_row();
        ui.label("Thinking reply");
        ui.text_edit_singleline(&mut app.e_thinking);
        ui.end_row();
    });
    ui.label(
        egui::RichText::new(
            "Wake reply = dit quand « hey jarvis » est entendu ; Thinking reply = quand tu as fini de \
             parler. Vide = juste un bip. (Sortie audio : relance l'app pour l'appliquer.)",
        )
        .small()
        .weak(),
    );

    ui.add_space(10.0);
    if ui.button("Save & test").clicked() {
        app.apply_settings();
    }
    if let Some((msg, ok)) = &app.save_msg {
        ui.colored_label(if *ok { GREEN } else { RED }, msg);
    }
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(
            "Endpoints + tokens persisted locally. Nothing is baked into the binary. \
             Health re-checks within ~5 s on the Dashboard.",
        )
        .small()
        .weak(),
    );
}
