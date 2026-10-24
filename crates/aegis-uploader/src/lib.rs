//! aegis-uploader — watcher de replays HotS → upload storm-codex. Copié de Hots-Overlay/client-rs
//! (detector/watcher/uploader), découplé de l'UI : `AppState.repaint` est un callback générique
//! (pas d'egui), et le serveur/token sont fournis par l'app (aegis-core::Config), pas gravés.
pub mod detector;
pub mod live;
pub mod settings;
pub mod state;
pub mod uploader;
pub mod watcher;

pub use state::{AppState, AppEvent, SharedState, EventSender, EventReceiver, WatcherStatus, UploadStatus, event_channel};
pub use live::LiveStatus;
