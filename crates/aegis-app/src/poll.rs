//! Poller d'arrière-plan : toutes les 5 s, sonde les services + GPU + now-playing → DashboardData.
//! Relit la config à chaque tick (les changements de Settings prennent effet en direct).
use crate::app::DashboardData;
use aegis_client::http::HttpClients;
use aegis_client::{GpuClient, HealthClient, MusicClient, TorrentsClient};
use aegis_core::{Config, ServiceHealth, ServiceStatus};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Notifie (toast bureau) un changement up↔down d'un service par rapport au tick précédent.
/// Le premier tick sert de référence (pas d'alerte au démarrage).
fn alert_transitions(prev: &mut HashMap<String, ServiceStatus>, health: &[ServiceHealth]) {
    let first = prev.is_empty();
    for h in health {
        let was = prev.insert(h.name.clone(), h.status.clone());
        if first {
            continue;
        }
        let down_now = h.status == ServiceStatus::Down;
        let was_down = was == Some(ServiceStatus::Down);
        let (changed, body) = match (was_down, down_now) {
            (false, true) => (true, format!("{} is down", h.name)),
            (true, false) if h.status == ServiceStatus::Up => (true, format!("{} recovered", h.name)),
            _ => (false, String::new()),
        };
        if changed {
            let _ = notify_rust::Notification::new().summary("Aegis").body(&body).show();
        }
    }
}

pub fn spawn_poller(
    rt: tokio::runtime::Handle,
    cfg: Arc<Mutex<Config>>,
    dash: Arc<Mutex<DashboardData>>,
    ctx: egui::Context,
) {
    rt.spawn(async move {
        let mut prev_status: HashMap<String, ServiceStatus> = HashMap::new();
        loop {
            let snapshot = cfg.lock().unwrap().clone();
            let clients = HttpClients::new(&snapshot);

            // Santé : Jarvis sur /health (gateway) ; storm-codex sur /api/health ; Orpheus sur
            // /api/auth/status.
            let mut probes = vec![
                ("Jarvis", snapshot.jarvis_url.as_str(), "/health"),
                ("storm-codex", snapshot.storm_codex_url.as_str(), "/api/health"),
                ("Orpheus", snapshot.orpheus_url.as_str(), "/api/auth/status"),
            ];
            // HotsPatchNotes : sondé seulement s'il est configuré (racine = best-effort).
            if let Some(hots) = snapshot.hots_patch_url.as_deref().filter(|s| !s.is_empty()) {
                probes.push(("HotsPatchNotes", hots, "/"));
            }
            let mut health = Vec::new();
            for (name, url, path) in probes {
                let r = clients.probe(url, path).await;
                let lat = r.clone().unwrap_or(0);
                health.push(ServiceHealth::from_probe(name, r.map(|_| ()), lat));
            }

            alert_transitions(&mut prev_status, &health);

            let gpu = clients.gpu().await;
            let now = clients.now_playing().await.unwrap_or_default();
            let torrents = clients.torrents().await.ok();

            {
                let mut d = dash.lock().unwrap();
                d.health = health;
                d.torrents = torrents;
                match gpu {
                    Ok(g) => {
                        d.gpu = Some(g);
                        d.gpu_error = None;
                    }
                    Err(e) => {
                        d.gpu = None;
                        d.gpu_error = Some(e);
                    }
                }
                d.now = now;
            }
            ctx.request_repaint();
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}
