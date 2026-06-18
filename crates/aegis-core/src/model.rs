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

#[derive(Debug, Clone, Default)]
pub struct ResidentModel {
    pub name: String,
    pub resident: bool,
    pub size_mb: u64,
}

#[derive(Debug, Clone, Default)]
pub struct GpuState {
    pub models: Vec<ResidentModel>,
    pub used_mb: u64,
    pub total_mb: u64,
}

impl GpuState {
    pub fn resident_count(&self) -> usize {
        self.models.iter().filter(|m| m.resident).count()
    }
    pub fn used_fraction(&self) -> f32 {
        if self.total_mb == 0 {
            0.0
        } else {
            self.used_mb as f32 / self.total_mb as f32
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NowPlaying {
    pub playing: bool,
    pub title: Option<String>,
    pub artist: Option<String>,
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
    fn gpu_summary_counts_resident() {
        let g = GpuState {
            models: vec![
                ResidentModel { name: "qwen".into(), resident: true, size_mb: 6600 },
                ResidentModel { name: "coder".into(), resident: false, size_mb: 0 },
            ],
            used_mb: 5800,
            total_mb: 8000,
        };
        assert_eq!(g.resident_count(), 1);
        assert!((g.used_fraction() - 0.725).abs() < 0.01);
    }
}
