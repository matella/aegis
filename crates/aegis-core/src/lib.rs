//! aegis-core — types, config et agrégation du tableau de bord. AUCUN I/O réseau ici (tout est
//! testable unitairement ; le réseau vit dans aegis-client derrière des traits mockables).
pub mod config;
pub mod model;

pub use config::{Config, ConfigFile};
pub use model::{
    Container, GpuState, NowPlaying, ResidentModel, ServiceHealth, ServiceStatus, TorrentItem,
    Torrents,
};
