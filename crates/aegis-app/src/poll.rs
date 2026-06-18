//! Poller d'arrière-plan : toutes les 5 s, sonde les services + GPU + now-playing → DashboardData.
//! Relit la config à chaque tick (les changements de Settings prennent effet en direct).
use crate::app::DashboardData;
use aegis_client::http::HttpClients;
use aegis_client::{GpuClient, HealthClient, MusicClient};
use aegis_core::{Config, ServiceHealth};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub fn spawn_poller(
    rt: tokio::runtime::Handle,
    cfg: Arc<Mutex<Config>>,
    dash: Arc<Mutex<DashboardData>>,
    ctx: egui::Context,
) {
    rt.spawn(async move {
        loop {
            let snapshot = cfg.lock().unwrap().clone();
            let clients = HttpClients::new(&snapshot);

            // Santé : Jarvis sur /health (gateway) ; storm-codex sur /api/health ; Orpheus sur
            // /api/auth/status.
            let probes = [
                ("Jarvis", snapshot.jarvis_url.as_str(), "/health"),
                ("storm-codex", snapshot.storm_codex_url.as_str(), "/api/health"),
                ("Orpheus", snapshot.orpheus_url.as_str(), "/api/auth/status"),
            ];
            let mut health = Vec::new();
            for (name, url, path) in probes {
                let r = clients.probe(url, path).await;
                let lat = r.clone().unwrap_or(0);
                health.push(ServiceHealth::from_probe(name, r.map(|_| ()), lat));
            }

            let gpu = clients.gpu().await;
            let now = clients.now_playing().await.unwrap_or_default();

            {
                let mut d = dash.lock().unwrap();
                d.health = health;
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
