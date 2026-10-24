//! Impls réelles du feed live : dossier sur disque (lecture partagée) + serveur via ureq, et la
//! boucle de poll qui fait tourner [`LiveWatcher`] et publie son statut dans [`AppState`].
use super::{ChunkReply, LiveDir, LiveSink, LiveStatus, LiveWatcher, PostError, Stream};
use crate::state::SharedState;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Cadence du poll. Le jeu écrit un bloc de 4 Ko toutes les ~10 s : 1 s suffit largement.
pub const POLL_INTERVAL: Duration = Duration::from_secs(1);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// `AEGIS_LIVE_DIR` si défini (tests, vérification sur un faux dossier), sinon
/// `%TEMP%\Heroes of the Storm\TempWriteReplayP1`.
pub fn default_live_dir() -> PathBuf {
    match std::env::var_os("AEGIS_LIVE_DIR") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => std::env::temp_dir().join("Heroes of the Storm").join("TempWriteReplayP1"),
    }
}

/// Le dossier `TempWriteReplayP1` sur disque.
pub struct FsLiveDir {
    root: PathBuf,
}

impl FsLiveDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Ouverture en lecture **partagée** (READ | WRITE | DELETE) : le jeu garde les fichiers ouverts
    /// en écriture pendant la partie.
    fn open(&self, name: &str) -> std::io::Result<File> {
        open_shared(&self.root.join(name))
    }
}

fn open_shared(path: &Path) -> std::io::Result<File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x1;
        const FILE_SHARE_WRITE: u32 = 0x2;
        const FILE_SHARE_DELETE: u32 = 0x4;
        opts.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE);
    }
    opts.open(path)
}

impl LiveDir for FsLiveDir {
    fn exists(&self) -> bool {
        self.root.is_dir()
    }
    fn has(&self, name: &str) -> bool {
        self.root.join(name).is_file()
    }
    /// Taille lue sur le handle ouvert, pas dans le listing du répertoire (périmé pour un fichier
    /// que le jeu garde ouvert).
    fn len(&self, name: &str) -> Option<u64> {
        let f = self.open(name).ok()?;
        f.metadata().ok().map(|m| m.len())
    }
    fn read_at(&self, name: &str, offset: u64, max: usize) -> std::io::Result<Vec<u8>> {
        let mut f = self.open(name)?;
        f.seek(SeekFrom::Start(offset))?;
        let mut out = Vec::new();
        f.take(max as u64).read_to_end(&mut out)?;
        Ok(out)
    }
    fn read_all(&self, name: &str) -> std::io::Result<Vec<u8>> {
        let mut out = Vec::new();
        self.open(name)?.read_to_end(&mut out)?;
        Ok(out)
    }
}

// ---- Codes HTTP → résultat (fonctions pures, testées) ----

fn rejected_or_transient(status: u16, body: &str) -> PostError {
    if (400..500).contains(&status) {
        PostError::Rejected(status, body.to_string())
    } else {
        PostError::Transient(format!("HTTP {status}: {}", body.trim()))
    }
}

/// `POST /api/live/chunk` : 200 (applied/duplicate/unreadable) → accepté ; 409 → `expected_offset`.
pub fn classify_chunk(status: u16, body: &str) -> Result<ChunkReply, PostError> {
    match status {
        200..=299 => Ok(ChunkReply::Accepted),
        409 => serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v.get("expected_offset")?.as_u64())
            .map(ChunkReply::Gap)
            .ok_or_else(|| PostError::Rejected(409, body.to_string())),
        _ => Err(rejected_or_transient(status, body)),
    }
}

/// `POST /api/lobby` : 202 (ingéré) / 200 (`unchanged`) → ok.
pub fn classify_lobby(status: u16, body: &str) -> Result<(), PostError> {
    match status {
        200..=299 => Ok(()),
        _ => Err(rejected_or_transient(status, body)),
    }
}

/// `POST /api/live/end` : 404 = ce n'est plus la session courante → rien à fermer, c'est fini.
pub fn classify_end(status: u16, body: &str) -> Result<(), PostError> {
    match status {
        200..=299 | 404 => Ok(()),
        _ => Err(rejected_or_transient(status, body)),
    }
}

/// Le serveur storm-codex (même transport que l'upload : `Bearer` du jeton d'upload).
pub struct HttpLiveSink {
    agent: ureq::Agent,
    pub server_url: String,
    pub token: Option<String>,
}

impl HttpLiveSink {
    pub fn new(server_url: String, token: Option<String>) -> Self {
        let agent = ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .timeout_global(Some(HTTP_TIMEOUT))
                .http_status_as_error(false)
                .build(),
        );
        Self { agent, server_url, token }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.server_url.trim_end_matches('/'))
    }

    fn post(
        &self,
        path: &str,
        content_type: &str,
        headers: &[(&str, String)],
        body: &[u8],
    ) -> Result<(u16, String), PostError> {
        let mut req = self.agent.post(self.url(path)).header("Content-Type", content_type);
        if let Some(t) = &self.token {
            req = req.header("Authorization", format!("Bearer {t}"));
        }
        for (k, v) in headers {
            req = req.header(*k, v);
        }
        match req.send(body) {
            Ok(mut resp) => {
                let status = resp.status().as_u16();
                let text = resp.body_mut().read_to_string().unwrap_or_default();
                Ok((status, text))
            }
            Err(e) => Err(PostError::Transient(format!("network: {e}"))),
        }
    }
}

impl LiveSink for HttpLiveSink {
    fn post_lobby(&mut self, bytes: &[u8]) -> Result<(), PostError> {
        let (st, body) = self.post("/api/lobby", "application/octet-stream", &[], bytes)?;
        classify_lobby(st, &body)
    }
    fn post_chunk(
        &mut self,
        session: &str,
        stream: Stream,
        offset: u64,
        bytes: &[u8],
    ) -> Result<ChunkReply, PostError> {
        let headers = [
            ("X-Session", session.to_string()),
            ("X-Stream", stream.header().to_string()),
            ("X-Offset", offset.to_string()),
        ];
        let (st, body) =
            self.post("/api/live/chunk", "application/octet-stream", &headers, bytes)?;
        classify_chunk(st, &body)
    }
    fn post_end(&mut self, session: &str) -> Result<(), PostError> {
        let body = serde_json::json!({ "session": session }).to_string();
        let (st, text) = self.post("/api/live/end", "application/json", &[], body.as_bytes())?;
        classify_end(st, &text)
    }
}

/// Arrête la boucle au drop (ou via [`stop`](Self::stop)).
pub struct LiveHandle {
    stop: Arc<AtomicBool>,
}

impl LiveHandle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for LiveHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Démarre **un** watcher live (une partie à la fois) sur un thread dédié : poll du dossier,
/// serveur/jeton relus dans l'état à chaque tick (modifiables dans Settings), statut publié dans
/// `AppState.live` (+ repaint au changement).
pub fn start_live_watcher(state: SharedState, dir: PathBuf) -> LiveHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    {
        let mut s = state.lock().unwrap();
        s.live_dir = dir.to_string_lossy().to_string();
    }
    std::thread::Builder::new()
        .name("aegis-live".into())
        .spawn(move || {
            let fs = FsLiveDir::new(dir);
            let mut watcher = LiveWatcher::new();
            let mut sink = HttpLiveSink::new(String::new(), None);
            while !flag.load(Ordering::Relaxed) {
                let (url, token) = {
                    let s = state.lock().unwrap();
                    (s.server_url.clone(), s.auth_token.clone())
                };
                let status = if url.trim().is_empty() {
                    LiveStatus::Idle
                } else {
                    sink.server_url = url;
                    sink.token = token;
                    watcher.tick(&fs, &mut sink)
                };
                publish(&state, status);
                std::thread::sleep(POLL_INTERVAL);
            }
        })
        .expect("spawn aegis-live thread");
    LiveHandle { stop }
}

fn publish(state: &SharedState, status: LiveStatus) {
    let mut s = state.lock().unwrap();
    if s.live != status {
        if s.live.label() != status.label() {
            crate::uploader::log(&format!("live: {} -> {:?}", s.live.label(), status));
        }
        s.live = status;
        s.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn chunk_codes() {
        assert_eq!(classify_chunk(200, r#"{"status":"applied"}"#), Ok(ChunkReply::Accepted));
        assert_eq!(classify_chunk(200, r#"{"status":"duplicate"}"#), Ok(ChunkReply::Accepted));
        assert_eq!(classify_chunk(200, r#"{"status":"unreadable"}"#), Ok(ChunkReply::Accepted));
        assert_eq!(
            classify_chunk(409, r#"{"status":"gap","expected_offset":8192}"#),
            Ok(ChunkReply::Gap(8192))
        );
        assert!(matches!(classify_chunk(409, "garbage"), Err(PostError::Rejected(409, _))));
        assert!(matches!(classify_chunk(401, ""), Err(PostError::Rejected(401, _))));
        assert!(matches!(classify_chunk(400, ""), Err(PostError::Rejected(400, _))));
        assert!(matches!(classify_chunk(413, ""), Err(PostError::Rejected(413, _))));
        assert!(matches!(classify_chunk(502, ""), Err(PostError::Transient(_))));
    }

    #[test]
    fn lobby_and_end_codes() {
        assert_eq!(classify_lobby(202, "{}"), Ok(()));
        assert_eq!(classify_lobby(200, r#"{"status":"unchanged"}"#), Ok(()));
        assert!(matches!(classify_lobby(401, ""), Err(PostError::Rejected(401, _))));
        assert_eq!(classify_end(200, ""), Ok(()));
        assert_eq!(classify_end(404, ""), Ok(()), "plus la session courante = rien à fermer");
        assert!(matches!(classify_end(500, ""), Err(PostError::Transient(_))));
    }

    #[test]
    fn fs_dir_reads_growing_file_held_open_by_writer() {
        let root = std::env::temp_dir().join(format!("aegis-live-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(Stream::Tracker.file());
        // Le « jeu » garde le fichier ouvert en écriture pendant qu'on lit.
        let mut writer = File::create(&path).unwrap();
        writer.write_all(&[1u8; 4096]).unwrap();
        writer.flush().unwrap();

        let dir = FsLiveDir::new(&root);
        assert!(dir.exists());
        assert!(dir.has(Stream::Tracker.file()));
        assert!(!dir.has(super::super::DETAILS));
        assert_eq!(dir.len(Stream::Tracker.file()), Some(4096));

        writer.write_all(&[2u8; 4096]).unwrap();
        writer.flush().unwrap();
        assert_eq!(dir.len(Stream::Tracker.file()), Some(8192));
        let tail = dir.read_at(Stream::Tracker.file(), 4096, 1 << 20).unwrap();
        assert_eq!(tail, vec![2u8; 4096]);
        assert_eq!(dir.read_at(Stream::Tracker.file(), 8192, 10).unwrap(), Vec::<u8>::new());
        assert_eq!(dir.read_at(Stream::Tracker.file(), 0, 10).unwrap(), vec![1u8; 10]);
        assert_eq!(dir.read_all(Stream::Tracker.file()).unwrap().len(), 8192);
        assert_eq!(dir.len("absent"), None);

        drop(writer);
        std::fs::remove_dir_all(&root).ok();
        assert!(!FsLiveDir::new(&root).exists());
    }

    #[test]
    fn live_dir_defaults_under_temp() {
        if std::env::var_os("AEGIS_LIVE_DIR").is_none() {
            let d = default_live_dir();
            assert!(d.ends_with(Path::new("Heroes of the Storm").join("TempWriteReplayP1")));
        }
    }
}
