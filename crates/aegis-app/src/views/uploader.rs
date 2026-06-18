//! Vue Uploader : statut du watcher + dossiers de replays + uploads récents. Wrappe aegis-uploader.
use crate::app::AegisApp;
use crate::views::{DIM, GREEN, RED};
use aegis_uploader::WatcherStatus;

pub fn ui(app: &mut AegisApp, ui: &mut egui::Ui) {
    ui.heading("Replay uploader");
    ui.add_space(6.0);

    let (status, count, connected, recent) = {
        let s = app.up_state.lock().unwrap();
        (
            s.watcher_status.clone(),
            s.uploaded_count,
            s.server_connected,
            s.recent_uploads
                .iter()
                .take(12)
                .map(|e| (e.filename.clone(), format!("{:?}", e.status)))
                .collect::<Vec<_>>(),
        )
    };

    ui.horizontal(|ui| {
        let (c, t) = match &status {
            WatcherStatus::Watching => (GREEN, "watching"),
            WatcherStatus::Stopped => (DIM, "stopped"),
            WatcherStatus::Error(_) => (RED, "error"),
        };
        ui.colored_label(c, "●");
        ui.label(t);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(if connected { "server: connected" } else { "server: offline" })
                    .small()
                    .color(DIM),
            );
        });
    });
    ui.label(format!("Uploaded: {count}"));

    ui.separator();
    ui.label(egui::RichText::new("REPLAY FOLDERS").small().color(DIM));
    let dirs = { app.up_state.lock().unwrap().replay_dirs.clone() };
    let mut to_remove = None;
    for (i, d) in dirs.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.monospace(d);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("✕").clicked() {
                    to_remove = Some(i);
                }
            });
        });
    }
    if dirs.is_empty() {
        ui.label(egui::RichText::new("no folders — add your Heroes of the Storm/Accounts path").color(DIM));
    }
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut app.new_dir);
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

    ui.separator();
    ui.label(egui::RichText::new("RECENT").small().color(DIM));
    if recent.is_empty() {
        ui.label(egui::RichText::new("none yet").color(DIM));
    }
    for (f, st) in recent {
        ui.horizontal(|ui| {
            ui.monospace(f);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(st).small().color(DIM));
            });
        });
    }
}

fn commit_dirs(app: &mut AegisApp, dirs: Vec<String>) {
    let _ = aegis_uploader::settings::save_dirs(&dirs);
    app.up_state.lock().unwrap().replay_dirs = dirs;
    app.restart_watchers();
}
