//! Vue chat Jarvis : historique + saisie. Le client WS (crate::chat) gère la connexion/reconnexion.
use crate::app::AegisApp;
use crate::audio;
use crate::chat::{ChatMsg, Outbound};
use crate::kit;
use crate::theme::{ACCENT, DIM, LINE, PANEL2, TEXT};
use crate::views::{GREEN, RED};

pub fn ui(app: &mut AegisApp, ui: &mut egui::Ui) {
    let (connected, wake) = {
        let s = app.chat.lock().unwrap();
        (s.connected, s.wake)
    };

    ui.horizontal(|ui| {
        super::page_title(ui, "Conversation");
        ui.add_space(8.0);
        kit::led(ui, if connected { GREEN } else { RED });
        ui.label(egui::RichText::new(if connected { "online" } else { "offline (gateway?)" }).size(11.0).color(DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Voix « hey Jarvis » : toggle micro (capture continue → gateway).
            let label = if app.listening {
                if wake { "🎙 listening…" } else { "🎤 mic on" }
            } else {
                "🎤 mic off"
            };
            if kit::toggle(ui, label, app.listening).clicked() {
                app.listening = !app.listening;
                if app.listening {
                    let mic = app.cfg.lock().unwrap().mic_device.clone();
                    app.listen_handle = audio::start_listening(app.chat_tx.clone(), mic);
                    if app.listen_handle.is_none() {
                        app.listening = false; // pas de micro / format non géré
                    }
                } else {
                    app.listen_handle = None; // drop → coupe la capture
                }
            }
            if app.listening {
                ui.label(egui::RichText::new("say \"hey jarvis\"").size(11.0).color(DIM));
            }
        });
    });
    ui.add_space(12.0);

    // Saisie ancrée en bas : champ + bouton accent « ↵ ».
    egui::TopBottomPanel::bottom("chat_input").show_separator_line(false).show_inside(ui, |ui| {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let btn_w = 40.0;
            let resp = ui.add_sized(
                [ui.available_width() - btn_w - 8.0, 30.0],
                egui::TextEdit::singleline(&mut app.chat_input).hint_text("Demande quelque chose à Jarvis…"),
            );
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let send = kit::btn_accent(ui, "▶").clicked() || enter;
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
        ui.add_space(6.0);
    });

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            let s = app.chat.lock().unwrap();
            if s.messages.is_empty() {
                ui.add_space(8.0);
                kit::dim(ui, "Ask Jarvis anything…");
            }
            for m in &s.messages {
                chat_line(ui, m);
                ui.add_space(8.0);
            }
            if s.thinking {
                ui.label(egui::RichText::new("jarvis is thinking…").color(DIM).italics());
            }
        });
}

/// Une ligne de chat : libellé locuteur (mono, largeur fixe) + bulle de texte.
fn chat_line(ui: &mut egui::Ui, m: &ChatMsg) {
    let who = if m.role == "you" { "you" } else { "jarvis" };
    ui.horizontal_top(|ui| {
        ui.add_sized([54.0, 18.0], egui::Label::new(kit::mono(who, ACCENT)));
        ui.vertical(|ui| {
            egui::Frame::new()
                .fill(PANEL2)
                .stroke(egui::Stroke::new(1.0, LINE))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(11, 8))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(&m.text).color(TEXT));
                    // Liens d'action (Jarvis a proposé/ouvert une page) → boutons.
                    for link in &m.links {
                        ui.add_space(4.0);
                        if ui.button(format!("↗ {}", link.title)).clicked() {
                            let _ = open::that(&link.url);
                        }
                    }
                });
        });
    });
}
