//! Vue Dashboard : santé des services + GPU/modèles + now-playing + actions rapides. Lit DashboardData
//! (alimenté par le poller) ; ne fait aucun fetch (sauf actions ponctuelles : Free GPU, ouvrir URL).
use crate::app::AegisApp;
use crate::views::{DIM, GREEN, RED};
use aegis_client::http::HttpClients;
use aegis_client::GpuClient;
use aegis_core::{GpuState, ServiceStatus};

pub fn ui(app: &mut AegisApp, ui: &mut egui::Ui) {
    ui.heading("Overview");
    ui.label(egui::RichText::new("auto-refresh 5 s").small().color(DIM));
    ui.add_space(8.0);

    let d = app.dash.lock().unwrap();

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(egui::RichText::new("SERVICES").small().color(DIM));
        if d.health.is_empty() {
            ui.label(egui::RichText::new("probing…").color(DIM));
        }
        for h in &d.health {
            ui.horizontal(|ui| {
                let (col, txt) = match h.status {
                    ServiceStatus::Up => (GREEN, format!("{} ms", h.latency_ms.unwrap_or(0))),
                    ServiceStatus::Down => (RED, "offline".to_string()),
                    ServiceStatus::Unknown => (DIM, "…".to_string()),
                };
                ui.colored_label(col, "●");
                ui.label(&h.name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(txt).small().color(DIM));
                });
            });
        }
    });

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(egui::RichText::new("GPU / MODELS").small().color(DIM));
        if let Some(g) = &d.gpu {
            ui.add(
                egui::ProgressBar::new(g.used_fraction(GpuState::RTX_2080S_TOTAL_GIB)).text(format!(
                    "{:.1} / {:.0} GiB",
                    g.vram_held_gib,
                    GpuState::RTX_2080S_TOTAL_GIB
                )),
            );
            if g.resident.is_empty() {
                ui.label(egui::RichText::new("no model resident").color(DIM));
            }
            for m in &g.resident {
                ui.colored_label(GREEN, format!("● {}  ({:.1} GiB, {})", m.model, m.vram_gib, m.processor));
            }
        } else if let Some(e) = &d.gpu_error {
            ui.label(egui::RichText::new(format!("unavailable: {e}")).color(DIM));
        } else {
            ui.label(egui::RichText::new("…").color(DIM));
        }
        if ui.button("Free GPU").clicked() {
            let (rt, cfg) = (app.rt.clone(), app.cfg.lock().unwrap().clone());
            rt.spawn(async move {
                let _ = HttpClients::new(&cfg).free().await;
            });
        }
    });

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(egui::RichText::new("NOW PLAYING").small().color(DIM));
        if d.now.playing {
            ui.label(format!(
                "♪ {} — {}",
                d.now.title.clone().unwrap_or_default(),
                d.now.artist.clone().unwrap_or_default()
            ));
        } else {
            ui.label(egui::RichText::new("nothing playing").color(DIM));
        }
    });

    drop(d);

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let cfg = app.cfg.lock().unwrap().clone();
        if ui.button("Open storm-codex ↗").clicked() {
            let _ = open::that(&cfg.storm_codex_url);
        }
        if ui.button("Jarvis console ↗").clicked() {
            let _ = open::that(&cfg.jarvis_url);
        }
        if ui.button("Patch notes ↗").clicked() {
            let _ = open::that(format!("{}/patches", cfg.storm_codex_url.trim_end_matches('/')));
        }
    });
}
