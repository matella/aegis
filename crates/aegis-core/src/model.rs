//! Modèle du tableau de bord + agrégation pure (pas d'I/O). Décide ce que l'UI affiche à partir des
//! résultats fournis par aegis-client (réels ou mock).

#[derive(Debug, Clone, PartialEq)]
pub enum ServiceStatus {
    Up,
    Down,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ServiceHealth {
    pub name: String,
    pub status: ServiceStatus,
    pub latency_ms: Option<u32>,
}

impl ServiceHealth {
    /// Construit l'état d'un service à partir d'un résultat de sonde. `Ok` = Up + latence ; `Err` = Down.
    pub fn from_probe(name: &str, res: Result<(), String>, latency_ms: u32) -> Self {
        match res {
            Ok(()) => Self {
                name: name.into(),
                status: ServiceStatus::Up,
                latency_ms: Some(latency_ms),
            },
            Err(_) => Self {
                name: name.into(),
                status: ServiceStatus::Down,
                latency_ms: None,
            },
        }
    }

    pub fn unknown(name: &str) -> Self {
        Self {
            name: name.into(),
            status: ServiceStatus::Unknown,
            latency_ms: None,
        }
    }
}

/// Un modèle résident en VRAM (forme de Jarvis `/api/gpu` → `gpu.gpu_status()`).
#[derive(Debug, Clone, Default)]
pub struct ResidentModel {
    pub model: String,
    pub vram_gib: f32,
    pub size_gib: f32,
    pub processor: String, // "GPU" | "GPU+CPU" | "CPU"
    pub expires_in_s: Option<u32>,
}

/// Occupation GPU telle que rapportée par la gateway Jarvis. Pas de VRAM totale (Ollama `/api/ps`
/// ne l'expose pas) ; le total matériel est une constante côté UI (RTX 2080 Super = 8 GiB).
#[derive(Debug, Clone, Default)]
pub struct GpuState {
    pub reachable: bool,
    pub resident: Vec<ResidentModel>,
    pub vram_held_gib: f32,
}

impl GpuState {
    pub const RTX_2080S_TOTAL_GIB: f32 = 8.0;

    pub fn model_count(&self) -> usize {
        self.resident.len()
    }
    /// Fraction de VRAM occupée sur un total donné (borne [0,1]).
    pub fn used_fraction(&self, total_gib: f32) -> f32 {
        if total_gib <= 0.0 {
            0.0
        } else {
            (self.vram_held_gib / total_gib).clamp(0.0, 1.0)
        }
    }
}

/// Un torrent actif (sous-ensemble du snapshot qBittorrent du gateway Jarvis `/api/torrents`).
#[derive(Debug, Clone)]
pub struct TorrentItem {
    pub name: String,
    pub progress: f32, // 0..1
    pub dl_speed: u64, // octets/s
}

/// Snapshot des téléchargements (gateway `/api/torrents` → qBittorrent).
#[derive(Debug, Clone, Default)]
pub struct Torrents {
    pub downloading: u32,
    pub seeding: u32,
    pub dl_speed: u64, // octets/s
    pub up_speed: u64,
    pub items: Vec<TorrentItem>,
}

#[derive(Debug, Clone, Default)]
pub struct NowPlaying {
    pub playing: bool,
    pub title: Option<String>,
    pub artist: Option<String>,
    /// URL de la pochette d'album (Spotify `track.album.images`), si disponible.
    pub cover_url: Option<String>,
    /// Progression de lecture (ms) et durée du titre (ms), pour la barre de progression.
    pub progress_ms: Option<u32>,
    pub duration_ms: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_down_when_probe_errs() {
        let h = ServiceHealth::from_probe("jarvis", Err("timeout".into()), 0);
        assert_eq!(h.status, ServiceStatus::Down);
        assert_eq!(h.latency_ms, None);
    }

    #[test]
    fn service_up_records_latency() {
        let h = ServiceHealth::from_probe("storm-codex", Ok(()), 12);
        assert_eq!(h.status, ServiceStatus::Up);
        assert_eq!(h.latency_ms, Some(12));
    }

    #[test]
    fn gpu_summary_counts_and_fraction() {
        let g = GpuState {
            reachable: true,
            resident: vec![ResidentModel {
                model: "qwen3.5:9b".into(),
                vram_gib: 6.0,
                size_gib: 6.0,
                processor: "GPU".into(),
                expires_in_s: Some(287),
            }],
            vram_held_gib: 6.0,
        };
        assert_eq!(g.model_count(), 1);
        assert!((g.used_fraction(8.0) - 0.75).abs() < 0.01);
    }

    #[test]
    fn gpu_fraction_clamps_and_handles_zero_total() {
        let g = GpuState { reachable: true, resident: vec![], vram_held_gib: 9.0 };
        assert_eq!(g.used_fraction(8.0), 1.0); // clamp
        assert_eq!(g.used_fraction(0.0), 0.0); // pas de division par zéro
    }
}
