//! Impls reqwest des traits clients (le seul code réseau). Pas de test unitaire (réseau) — couvert
//! par la checklist de run manuel ; la logique (chemins, mapping d'erreurs) reste triviale.
use crate::parse::{parse_gpu, parse_now_playing, parse_torrents};
use crate::{GpuClient, HealthClient, MusicClient, TorrentsClient};
use aegis_core::{Config, GpuState, NowPlaying, Torrents};
use async_trait::async_trait;
use std::time::{Duration, Instant};

/// Clients réseau construits à partir de la config courante.
pub struct HttpClients {
    http: reqwest::Client,
    jarvis_url: String,
    jarvis_token: Option<String>,
    orpheus_url: String,
}

impl HttpClients {
    pub fn new(cfg: &Config) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_default();
        Self {
            http,
            jarvis_url: cfg.jarvis_url.trim_end_matches('/').to_string(),
            jarvis_token: cfg.jarvis_token.clone(),
            orpheus_url: cfg.orpheus_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl HealthClient for HttpClients {
    /// `health_path` varie selon le service (storm-codex/jarvis : `/api/health` ; Orpheus :
    /// `/api/auth/status`). Renvoie la latence en ms si 2xx.
    async fn probe(&self, url: &str, health_path: &str) -> Result<u32, String> {
        let full = format!("{}{}", url.trim_end_matches('/'), health_path);
        let t0 = Instant::now();
        let res = self.http.get(&full).send().await.map_err(|e| e.to_string())?;
        if res.status().is_success() {
            Ok(t0.elapsed().as_millis() as u32)
        } else {
            Err(format!("HTTP {}", res.status().as_u16()))
        }
    }
}

#[async_trait]
impl GpuClient for HttpClients {
    async fn gpu(&self) -> Result<GpuState, String> {
        let mut req = self.http.get(format!("{}/api/gpu", self.jarvis_url));
        if let Some(tok) = &self.jarvis_token {
            req = req.bearer_auth(tok);
        }
        let body = req
            .send()
            .await
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        parse_gpu(&body)
    }
    async fn free(&self) -> Result<(), String> {
        let mut req = self.http.post(format!("{}/api/gpu/free", self.jarvis_url));
        if let Some(tok) = &self.jarvis_token {
            req = req.bearer_auth(tok);
        }
        let res = req.send().await.map_err(|e| e.to_string())?;
        res.error_for_status().map(|_| ()).map_err(|e| e.to_string())
    }
}

#[async_trait]
impl MusicClient for HttpClients {
    async fn now_playing(&self) -> Result<NowPlaying, String> {
        let body = self
            .http
            .get(format!("{}/api/playback/now", self.orpheus_url))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        parse_now_playing(&body)
    }
}

#[async_trait]
impl TorrentsClient for HttpClients {
    async fn torrents(&self) -> Result<Torrents, String> {
        let mut req = self.http.get(format!("{}/api/torrents", self.jarvis_url));
        if let Some(tok) = &self.jarvis_token {
            req = req.bearer_auth(tok);
        }
        let res = req.send().await.map_err(|e| e.to_string())?;
        if !res.status().is_success() {
            return Err(format!("HTTP {}", res.status().as_u16()));
        }
        let body = res.text().await.map_err(|e| e.to_string())?;
        parse_torrents(&body)
    }
}
