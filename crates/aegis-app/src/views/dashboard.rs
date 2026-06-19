//! Vue Dashboard : santé des services + GPU/modèles + now-playing + actions rapides. Lit DashboardData
//! (alimenté par le poller) ; ne fait aucun fetch (sauf actions ponctuelles : Free GPU, ouvrir URL).
use crate::app::AegisApp;
use crate::kit;
use crate::theme::{ACCENT, DIM, TEXT};
use crate::views::{GREEN, RED};
use aegis_client::http::HttpClients;
use aegis_client::GpuClient;
use aegis_core::{GpuState, ServiceStatus};

pub fn ui(app: &mut AegisApp, ui: &mut egui::Ui) {
    super::header(ui, "Overview", "auto-refresh 5s");

    let d = app.dash.lock().unwrap();

    // Grille 2×2 du mockup : Services | GPU, puis Now playing | Quick actions.
    ui.columns(2, |c| {
        services_card(&mut c[0], &d.health);
        gpu_card(&mut c[1], app, &d);
    });
    ui.add_space(12.0);
    let now = d.now.clone();
    drop(d);
    let cfg = app.cfg.lock().unwrap().clone();
    ui.columns(2, |c| {
        now_playing_card(&mut c[0], &now);
        quick_actions_card(&mut c[1], &cfg);
    });
}

fn services_card(ui: &mut egui::Ui, health: &[aegis_core::ServiceHealth]) {
    kit::card_titled(ui, "Services", |ui| {
        if health.is_empty() {
            kit::dim(ui, "probing…");
        }
        for h in health {
            ui.horizontal(|ui| {
                let (col, txt) = match h.status {
                    ServiceStatus::Up => (GREEN, format!("{} ms", h.latency_ms.unwrap_or(0))),
                    ServiceStatus::Down => (RED, "offline".to_string()),
                    ServiceStatus::Unknown => (DIM, "…".to_string()),
                };
                kit::led(ui, col);
                ui.label(egui::RichText::new(&h.name).color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(kit::mono(txt, if h.status == ServiceStatus::Down { RED } else { DIM }));
                });
            });
            ui.add_space(5.0);
        }
    });
}

fn gpu_card(ui: &mut egui::Ui, app: &AegisApp, d: &crate::app::DashboardData) {
    kit::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("GPU · RTX 2080 Super").size(13.0).strong().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(g) = &d.gpu {
                    kit::badge(
                        ui,
                        &format!("{:.1} / {:.0} GB", g.vram_held_gib, GpuState::RTX_2080S_TOTAL_GIB),
                        DIM,
                    );
                }
            });
        });
        ui.add_space(8.0);

        if let Some(g) = &d.gpu {
            let frac = g.used_fraction(GpuState::RTX_2080S_TOTAL_GIB) as f32;
            kit::gauge(ui, frac, if frac > 0.85 { RED } else { ACCENT });
            ui.add_space(8.0);
            if g.resident.is_empty() {
                kit::dim(ui, "no model resident");
            }
            for m in &g.resident {
                ui.horizontal(|ui| {
                    kit::led(ui, GREEN);
                    ui.label(egui::RichText::new(&m.model).color(TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(kit::mono(format!("{:.1} GiB · {}", m.vram_gib, m.processor), DIM));
                    });
                });
                ui.add_space(3.0);
            }
        } else if let Some(e) = &d.gpu_error {
            kit::dim(ui, &format!("unavailable: {e}"));
        } else {
            kit::dim(ui, "…");
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Free GPU").clicked() {
                let (rt, cfg) = (app.rt.clone(), app.cfg.lock().unwrap().clone());
                rt.spawn(async move {
                    let _ = HttpClients::new(&cfg).free().await;
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let n = d.gpu.as_ref().map(|g| g.resident.len()).unwrap_or(0);
                ui.label(egui::RichText::new(format!("{n} model resident")).size(11.0).color(DIM));
            });
        });
    });
}

fn now_playing_card(ui: &mut egui::Ui, now: &aegis_core::NowPlaying) {
    kit::card_titled(ui, "Now playing", |ui| {
        if now.playing {
            ui.horizontal(|ui| {
                // Pochette : image distante (Spotify) si dispo, sinon carré + note 🎵.
                let sz = egui::vec2(46.0, 46.0);
                match &now.cover_url {
                    Some(url) => {
                        ui.add(
                            egui::Image::new(url)
                                .fit_to_exact_size(sz)
                                .corner_radius(6)
                                .bg_fill(egui::Color32::from_rgb(0x47, 0x3a, 0x6b)),
                        );
                    }
                    None => {
                        let (rect, _) = ui.allocate_exact_size(sz, egui::Sense::hover());
                        let p = ui.painter();
                        p.rect_filled(rect, egui::CornerRadius::same(6), egui::Color32::from_rgb(0x47, 0x3a, 0x6b));
                        let note = p.layout_no_wrap("🎵".to_owned(), egui::FontId::proportional(24.0), egui::Color32::WHITE);
                        p.galley(rect.center() - note.size() / 2.0, note, egui::Color32::WHITE);
                    }
                }
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(now.title.clone().unwrap_or_default()).strong().color(TEXT),
                    );
                    ui.label(egui::RichText::new(now.artist.clone().unwrap_or_default()).size(12.0).color(DIM));
                    if let (Some(pos), Some(dur)) = (now.progress_ms, now.duration_ms) {
                        if dur > 0 {
                            ui.add_space(4.0);
                            ui.label(kit::mono(format!("{} / {}", fmt_ms(pos), fmt_ms(dur)), DIM));
                            ui.add_space(2.0);
                            kit::gauge(ui, pos as f32 / dur as f32, ACCENT);
                        }
                    }
                });
            });
        } else {
            kit::dim(ui, "nothing playing");
        }
    });
}

/// Millisecondes → `m:ss`.
fn fmt_ms(ms: u32) -> String {
    let total = ms / 1000;
    format!("{}:{:02}", total / 60, total % 60)
}

fn quick_actions_card(ui: &mut egui::Ui, cfg: &aegis_core::Config) {
    kit::card_titled(ui, "Quick actions", |ui| {
        ui.horizontal_wrapped(|ui| {
            if kit::btn_accent(ui, "Open storm-codex ↗").clicked() {
                let _ = open::that(&cfg.storm_codex_url);
            }
            if ui.button("Jarvis console ↗").clicked() {
                let _ = open::that(&cfg.jarvis_url);
            }
            if ui.button("Patch notes ↗").clicked() {
                let _ = open::that(format!("{}/patches", cfg.storm_codex_url.trim_end_matches('/')));
            }
            if let Some(url) = cfg.homelab_url.as_deref().filter(|s| !s.is_empty()) {
                if ui.button("Homelab ↗").clicked() {
                    let _ = open::that(url);
                }
            }
        });
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Ouvre les UIs web dans le navigateur (egui = pas d'embarquage).")
                .size(11.0)
                .color(DIM),
        );
    });
}
