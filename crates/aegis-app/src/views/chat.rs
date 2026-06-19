//! Vue chat Jarvis : historique + saisie. Le client WS (crate::chat) gère la connexion/reconnexion.
use crate::app::AegisApp;
use crate::audio;
use crate::chat::{ChatMsg, Outbound};
use crate::kit;
use crate::theme::{ACCENT, AMBER, DIM, LINE, PANEL2, TEXT};
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
                    s.messages.push(ChatMsg { role: "you".into(), text: text.clone(), ..Default::default() });
                    s.thinking = true;
                }
                let _ = app.chat_tx.send(Outbound::Text(text));
                resp.request_focus();
            }
        });
        ui.add_space(6.0);
    });

    // Le rendu peut produire une décision de confirmation (Confirm/Reject d'une action gated).
    // On la collecte pendant le rendu (verrou tenu) puis on l'envoie après libération du verrou.
    let decision = egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            let s = app.chat.lock().unwrap();
            if s.messages.is_empty() {
                ui.add_space(8.0);
                kit::dim(ui, "Ask Jarvis anything…");
            }
            let last = s.messages.len().saturating_sub(1);
            let mut decision: Option<&'static str> = None;
            for (i, m) in s.messages.iter().enumerate() {
                // Seule la dernière proposition est « live » (le pending côté gateway).
                if let Some(d) = chat_line(ui, m, i == last) {
                    decision = Some(d);
                }
                ui.add_space(8.0);
            }
            if s.thinking {
                ui.label(egui::RichText::new("jarvis is thinking…").color(DIM).italics());
            }
            decision
        })
        .inner;

    // Confirm/Reject → on renvoie « confirm »/« reject » sur le /ws (mots affirmatif/négatif que
    // la gateway mappe sur l'intent en attente), comme une réponse vocale.
    if let Some(word) = decision {
        {
            let mut s = app.chat.lock().unwrap();
            s.messages.push(ChatMsg { role: "you".into(), text: word.into(), ..Default::default() });
            s.thinking = true;
        }
        let _ = app.chat_tx.send(Outbound::Text(word.into()));
    }
}

/// Une ligne de chat : libellé locuteur (mono, largeur fixe) + bulle de texte. Pour une action
/// gated proposée (`m.proposal`) et « live » (`live`), affiche une rangée de confirmation ambre et
/// renvoie « confirm »/« reject » si l'opérateur clique.
fn chat_line(ui: &mut egui::Ui, m: &ChatMsg, live: bool) -> Option<&'static str> {
    let who = if m.role == "you" { "you" } else { "jarvis" };
    let mut decision = None;
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
            // Action gated : rangée de confirmation (bordure ambre, façon mockup #4).
            if m.proposal && live {
                ui.add_space(6.0);
                egui::Frame::new()
                    .stroke(egui::Stroke::new(1.0, AMBER.gamma_multiply(0.6)))
                    .corner_radius(egui::CornerRadius::same(7))
                    .inner_margin(egui::Margin::symmetric(11, 7))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("⚠ confirmation requise").size(11.0).color(AMBER));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("Reject").clicked() {
                                    decision = Some("reject");
                                }
                                if kit::btn_accent(ui, "Confirm").clicked() {
                                    decision = Some("confirm");
                                }
                            });
                        });
                    });
            }
        });
    });
    decision
}
