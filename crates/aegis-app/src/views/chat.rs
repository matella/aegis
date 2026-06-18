//! Vue chat Jarvis : historique + saisie. Le client WS (crate::chat) gère la connexion/reconnexion.
use crate::app::AegisApp;
use crate::audio;
use crate::chat::{ChatMsg, Outbound};
use crate::views::{DIM, GREEN, RED};

const YOU: egui::Color32 = egui::Color32::from_rgb(126, 151, 255);

pub fn ui(app: &mut AegisApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("Jarvis");
        let (connected, wake) = {
            let s = app.chat.lock().unwrap();
            (s.connected, s.wake)
        };
        ui.colored_label(
            if connected { GREEN } else { RED },
            if connected { "● online" } else { "○ offline (gateway?)" },
        );
        // Voix « hey Jarvis » : toggle micro (capture continue → gateway).
        let label = if app.listening {
            if wake { "🎙 listening…" } else { "🎤 mic on" }
        } else {
            "🎤 mic off"
        };
        if ui.selectable_label(app.listening, label).clicked() {
            app.listening = !app.listening;
            if app.listening {
                app.listen_handle = audio::start_listening(app.chat_tx.clone());
                if app.listen_handle.is_none() {
                    app.listening = false; // pas de micro / format non géré
                }
            } else {
                app.listen_handle = None; // drop → coupe la capture
            }
        }
        if app.listening {
            ui.label(egui::RichText::new("say \"hey jarvis\"").small().color(DIM));
        }
    });
    ui.add_space(4.0);

    // Saisie ancrée en bas ; l'historique remplit le reste.
    egui::TopBottomPanel::bottom("chat_input").show_inside(ui, |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let resp = ui.add_sized(
                [ui.available_width() - 64.0, 28.0],
                egui::TextEdit::singleline(&mut app.chat_input).hint_text("Message Jarvis…"),
            );
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let send = ui.button("Send").clicked() || enter;
            if send && !app.chat_input.trim().is_empty() {
                let text = app.chat_input.trim().to_string();
                app.chat_input.clear();
                {
                    let mut s = app.chat.lock().unwrap();
                    s.messages.push(ChatMsg { role: "you".into(), text: text.clone(), links: Vec::new() });
                    s.thinking = true;
                }
                let _ = app.chat_tx.send(Outbound::Text(text));
                resp.request_focus();
            }
        });
        ui.add_space(4.0);
    });

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            let s = app.chat.lock().unwrap();
            if s.messages.is_empty() {
                ui.label(egui::RichText::new("Ask Jarvis anything…").color(DIM));
            }
            for m in &s.messages {
                let (who, col) = if m.role == "you" { ("you", YOU) } else { ("jarvis", GREEN) };
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(format!("{who}  ")).color(col).strong());
                    ui.label(&m.text);
                });
                // Liens d'action (Jarvis a proposé/ouvert une page) → boutons.
                for link in &m.links {
                    if ui.button(format!("↗ {}", link.title)).clicked() {
                        let _ = open::that(&link.url);
                    }
                }
                ui.add_space(6.0);
            }
            if s.thinking {
                ui.label(egui::RichText::new("jarvis is thinking…").color(DIM).italics());
            }
        });
}
