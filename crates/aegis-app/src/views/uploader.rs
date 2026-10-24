//! Vue Uploader : statut du watcher + dossiers de replays + uploads récents. Wrappe aegis-uploader.
use crate::app::AegisApp;
use crate::kit;
use crate::theme::{AMBER, DIM, TEXT};
use crate::views::{GREEN, RED};
use aegis_uploader::{LiveStatus, WatcherStatus};

pub fn ui(app: &mut AegisApp, ui: &mut egui::Ui) {
    let (status, count, connected, recent) = {
        let s = app.up_state.lock().unwrap();
        (
            s.watcher_status.clone(),
            s.uploaded_count,
            s.server_connected,
            s.recent_uploads
                .iter()
                .take(12)
                .map(|e| (e.filename.clone(), format!("{:?}", e.status), e.error.clone()))
                .collect::<Vec<_>>(),
        )
    };

    // En-tête : titre + badge d'état du watcher à droite (façon mockup).
    ui.horizontal(|ui| {
        super::page_title(ui, "Replay uploader");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (c, t) = match &status {
                WatcherStatus::Watching => (GREEN, "watching"),
                WatcherStatus::Stopped => (DIM, "stopped"),
                WatcherStatus::Error(_) => (RED, "error"),
            };
            kit::led(ui, c);
            kit::badge(ui, t, c);
        });
    });
    ui.add_space(12.0);

    // Carte de stats : lignes label/valeur étalées.
    kit::card(ui, |ui| {
        spread(ui, "Server", if connected { "connected" } else { "offline" }, if connected { GREEN } else { DIM });
        ui.add_space(6.0);
        spread(ui, "Uploaded", &count.to_string(), TEXT);
    });

    ui.add_space(12.0);
    live_card(app, ui);
    ui.add_space(12.0);

    kit::card_titled(ui, "Replay folders", |ui| {
        let dirs = { app.up_state.lock().unwrap().replay_dirs.clone() };
        let mut to_remove = None;
        if dirs.is_empty() {
            kit::dim(ui, "no folders — add your Heroes of the Storm/Accounts path");
        }
        for (i, d) in dirs.iter().enumerate() {
            if i > 0 {
                kit::hairline(ui);
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(d).monospace().color(TEXT).size(11.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✖").clicked() {
                        to_remove = Some(i);
                    }
                });
            });
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut app.new_dir).hint_text("path to replay folder…"));
            let add = ui.button("Add").clicked() && !app.new_dir.trim().is_empty();
            let detect = ui.button("Auto-detect").clicked();
            if add {
                let mut next = dirs.clone();
                next.push(app.new_dir.trim().to_string());
                app.new_dir.clear();
                commit_dirs(app, next);
            } else if detect {
                let mut next = dirs.clone();
                for p in aegis_uploader::detector::detect_replay_dirs() {
                    let s = p.to_string_lossy().to_string();
                    if !next.contains(&s) {
                        next.push(s);
                    }
                }
                commit_dirs(app, next);
            }
        });
        if let Some(i) = to_remove {
            let mut next = dirs.clone();
            next.remove(i);
            commit_dirs(app, next);
        }
    });

    ui.add_space(14.0);
    kit::cap(ui, "Recent");
    ui.add_space(6.0);
    kit::card(ui, |ui| {
        if recent.is_empty() {
            kit::dim(ui, "none yet");
        }
        for (i, (f, st, err)) in recent.iter().enumerate() {
            if i > 0 {
                kit::hairline(ui);
            }
            let failed = st.eq_ignore_ascii_case("failed");
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(f).monospace().color(TEXT).size(11.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ok = st.eq_ignore_ascii_case("success")
                        || st.eq_ignore_ascii_case("uploaded")
                        || st.eq_ignore_ascii_case("duplicate");
                    let color = if ok { GREEN } else if failed { RED } else { DIM };
                    let badge = kit::badge(ui, &st.to_lowercase(), color);
                    if let Some(e) = err {
                        badge.on_hover_text(e); // détail complet de l'erreur au survol
                    }
                });
            });
            // Sous-ligne rouge tronquée : la raison de l'échec (ex. « HTTP 401: unauthorized »).
            if failed {
                if let Some(e) = err {
                    let short: String = e.chars().take(80).collect();
                    ui.label(egui::RichText::new(short).size(10.0).color(RED)).on_hover_text(e);
                }
            }
        }
    });
}

/// Feed live de la partie en cours (storm-codex `/live`) : idle · streaming · ended · error.
fn live_card(app: &AegisApp, ui: &mut egui::Ui) {
    let (live, dir) = {
        let s = app.up_state.lock().unwrap();
        (s.live.clone(), s.live_dir.clone())
    };
    let color = match &live {
        LiveStatus::Streaming { warning: Some(_), .. } => AMBER,
        LiveStatus::Streaming { .. } => GREEN,
        LiveStatus::Error(_) => RED,
        LiveStatus::Idle | LiveStatus::Ended { .. } => DIM,
    };
    kit::card_titled(ui, "Live game", |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Status").color(DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                kit::badge(ui, live.label(), color);
                kit::led(ui, color);
            });
        });
        match &live {
            LiveStatus::Streaming { session, tracker, game, warning } => {
                ui.add_space(6.0);
                spread(ui, "Session", &short(session), TEXT);
                ui.add_space(6.0);
                spread(ui, "Sent", &format!("tracker {tracker} · game {game}"), TEXT);
                if let Some(w) = warning {
                    let short_w: String = w.chars().take(80).collect();
                    ui.label(egui::RichText::new(format!("server unreachable, will catch up — {short_w}")).size(10.0).color(AMBER))
                        .on_hover_text(w);
                }
            }
            LiveStatus::Ended { session } => {
                ui.add_space(6.0);
                spread(ui, "Session", &short(session), DIM);
            }
            LiveStatus::Error(e) => {
                let short_e: String = e.chars().take(80).collect();
                ui.label(egui::RichText::new(short_e).size(10.0).color(RED)).on_hover_text(e);
            }
            LiveStatus::Idle => {}
        }
        ui.add_space(6.0);
        spread(ui, "Folder", &dir, DIM);
    });
}

fn short(session: &str) -> String {
    format!("{}…", &session[..session.len().min(12)])
}

/// Ligne « label sourd à gauche · valeur à droite » (lignes de la carte de stats).
fn spread(ui: &mut egui::Ui, label: &str, value: &str, value_color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(kit::mono(value, value_color));
        });
    });
}

fn commit_dirs(app: &mut AegisApp, dirs: Vec<String>) {
    let _ = aegis_uploader::settings::save_dirs(&dirs);
    app.up_state.lock().unwrap().replay_dirs = dirs;
    app.restart_watchers();
}
