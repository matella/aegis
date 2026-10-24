//! État partagé du watcher/uploader. Identique à client-rs SAUF le repaint : ici un callback
//! générique `Arc<dyn Fn()>` (au lieu d'`egui::Context`) → la crate ne dépend pas de l'UI.
use chrono::{DateTime, Local};
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

const MAX_RECENT: usize = 50;

#[derive(Debug, Clone, PartialEq)]
pub enum UploadStatus {
    Pending,
    Success,
    Failed,
    Duplicate,
    Skipped,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WatcherStatus {
    Stopped,
    Watching,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub filename: String,
    pub status: UploadStatus,
    pub timestamp: DateTime<Local>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BulkProgress {
    pub done: usize,
    pub failed: usize,
    pub duplicates: usize,
    pub total: usize,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AppEvent {
    ServerConnected,
    ServerDisconnected,
    WatcherStarted,
    WatcherStopped,
    WatcherError(String),
    UploadStart { filename: String },
    UploadSuccess { filename: String },
    UploadFailed { filename: String, error: String },
    UploadDuplicate { filename: String },
    BulkProgress(BulkProgress),
    BulkDone,
}

/// Callback de repaint (l'app y branche son ctx egui ; ici on ne connaît pas l'UI).
pub type RepaintFn = Arc<dyn Fn() + Send + Sync>;

pub struct AppState {
    pub server_url: String,
    pub auth_token: Option<String>,
    pub server_connected: bool,

    pub watcher_status: WatcherStatus,
    pub replay_dirs: Vec<String>,

    pub uploaded: HashSet<String>,
    pub uploaded_count: usize,
    pub recent_uploads: VecDeque<LogEntry>,
    pub bulk_progress: Option<BulkProgress>,
    pub scanning: usize,

    /// Feed live de la partie en cours (storm-codex `/live`) : statut + dossier surveillé.
    pub live: crate::live::LiveStatus,
    pub live_dir: String,

    /// Callback de repaint (None tant que l'app ne l'a pas branché).
    pub repaint: Option<RepaintFn>,
}

impl AppState {
    pub fn new(
        server_url: String,
        auth_token: Option<String>,
        replay_dirs: Vec<String>,
        uploaded: HashSet<String>,
    ) -> Self {
        let uploaded_count = uploaded.len();
        Self {
            server_url,
            auth_token,
            server_connected: false,
            watcher_status: WatcherStatus::Stopped,
            replay_dirs,
            uploaded,
            uploaded_count,
            recent_uploads: VecDeque::new(),
            bulk_progress: None,
            scanning: 0,
            live: crate::live::LiveStatus::Idle,
            live_dir: String::new(),
            repaint: None,
        }
    }

    pub fn add_recent(&mut self, filename: String, status: UploadStatus) {
        self.recent_uploads.push_front(LogEntry {
            filename,
            status,
            timestamp: Local::now(),
            error: None,
        });
        if self.recent_uploads.len() > MAX_RECENT {
            self.recent_uploads.pop_back();
        }
    }

    pub fn update_recent(&mut self, filename: &str, status: UploadStatus) {
        if let Some(entry) = self.recent_uploads.iter_mut().find(|e| e.filename == filename) {
            entry.status = status;
        } else {
            self.add_recent(filename.to_string(), status);
        }
    }

    pub fn update_recent_with_error(&mut self, filename: &str, status: UploadStatus, error: String) {
        if let Some(entry) = self.recent_uploads.iter_mut().find(|e| e.filename == filename) {
            entry.status = status;
            entry.error = Some(error);
        } else {
            self.recent_uploads.push_front(LogEntry {
                filename: filename.to_string(),
                status,
                timestamp: Local::now(),
                error: Some(error),
            });
            if self.recent_uploads.len() > MAX_RECENT {
                self.recent_uploads.pop_back();
            }
        }
    }

    pub fn request_repaint(&self) {
        if let Some(f) = &self.repaint {
            f();
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;
pub type EventSender = mpsc::UnboundedSender<AppEvent>;
pub type EventReceiver = mpsc::UnboundedReceiver<AppEvent>;

pub fn event_channel() -> (EventSender, EventReceiver) {
    mpsc::unbounded_channel()
}
