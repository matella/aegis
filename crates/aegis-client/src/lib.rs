//! aegis-client — SEUL endroit qui parle HTTP. Traits async + impls mock (tests sans box) + impls
//! reqwest (`http`). Les parseurs purs vivent dans `parse`.
pub mod http;
pub mod parse;

use aegis_core::{GpuState, NowPlaying};
use async_trait::async_trait;

/// Sonde de santé d'un service → latence en ms (Err = down).
#[async_trait]
pub trait HealthClient: Send + Sync {
    async fn probe(&self, url: &str, health_path: &str) -> Result<u32, String>;
}

/// Occupation GPU via la gateway Jarvis (`/api/gpu`, auth bearer) + éviction (`/api/gpu/free`).
#[async_trait]
pub trait GpuClient: Send + Sync {
    async fn gpu(&self) -> Result<GpuState, String>;
    async fn free(&self) -> Result<(), String>;
}

/// Lecture en cours (Orpheus / proxy storm-codex).
#[async_trait]
pub trait MusicClient: Send + Sync {
    async fn now_playing(&self) -> Result<NowPlaying, String>;
}

// ── Mocks (pour les tests de la logique dashboard/poller, sans réseau) ──────────────────────────
pub struct MockGpu(pub Result<GpuState, String>);
#[async_trait]
impl GpuClient for MockGpu {
    async fn gpu(&self) -> Result<GpuState, String> {
        self.0.clone()
    }
    async fn free(&self) -> Result<(), String> {
        Ok(())
    }
}

pub struct MockMusic(pub NowPlaying);
#[async_trait]
impl MusicClient for MockMusic {
    async fn now_playing(&self) -> Result<NowPlaying, String> {
        Ok(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::ResidentModel;

    #[tokio::test]
    async fn mock_gpu_returns_state() {
        let m = MockGpu(Ok(GpuState {
            reachable: true,
            resident: vec![ResidentModel { model: "q".into(), vram_gib: 6.0, size_gib: 6.0, processor: "GPU".into(), expires_in_s: None }],
            vram_held_gib: 6.0,
        }));
        let g = m.gpu().await.unwrap();
        assert_eq!(g.model_count(), 1);
        assert!(m.free().await.is_ok());
    }

    #[tokio::test]
    async fn mock_music_not_playing() {
        let m = MockMusic(NowPlaying::default());
        assert!(!m.now_playing().await.unwrap().playing);
    }
}
