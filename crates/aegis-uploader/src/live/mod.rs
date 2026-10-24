//! Feed live d'une partie en cours → storm-codex `/live` (contrat : `docs/spec/05-api.md`
//! « Feed live » de storm-codex). Le jeu écrit `%TEMP%\Heroes of the Storm\TempWriteReplayP1\`
//! pendant la partie ; on pousse **les octets bruts** (le parse reste côté serveur) :
//! - `replay.server.battlelobby` → `POST /api/lobby` (une fois par contenu) ;
//! - `replay.tracker.events` / `replay.game.events` → `POST /api/live/chunk` (octets ajoutés,
//!   `X-Session` = SHA-256 hex du battlelobby, `X-Stream`, `X-Offset`) ;
//! - apparition de `replay.details` + `replay.initData` → dernier flush puis `POST /api/live/end`.
//!
//! La logique est une machine à états pure ([`LiveWatcher::tick`]) derrière deux traits mockables :
//! [`LiveDir`] (le dossier) et [`LiveSink`] (le serveur). Les impls réelles sont dans [`io`].
pub mod io;

use sha2::{Digest, Sha256};

pub const LOBBY: &str = "replay.server.battlelobby";
pub const DETAILS: &str = "replay.details";
pub const INIT_DATA: &str = "replay.initData";

/// Taille max d'un POST de morceau : borne le rattrapage après une coupure (le jeu écrit par 4 Ko).
pub const MAX_CHUNK: usize = 256 * 1024;
/// Garde-fou : au plus N POST par stream et par tick (un serveur qui renverrait des 409 en boucle).
const MAX_POSTS_PER_TICK: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Tracker,
    Game,
}

impl Stream {
    pub const ALL: [Stream; 2] = [Stream::Tracker, Stream::Game];
    /// Valeur de l'en-tête `X-Stream`.
    pub fn header(self) -> &'static str {
        match self {
            Stream::Tracker => "tracker",
            Stream::Game => "game",
        }
    }
    pub fn file(self) -> &'static str {
        match self {
            Stream::Tracker => "replay.tracker.events",
            Stream::Game => "replay.game.events",
        }
    }
    fn idx(self) -> usize {
        match self {
            Stream::Tracker => 0,
            Stream::Game => 1,
        }
    }
}

/// Le dossier `TempWriteReplayP1` (lecture seule). Tailles et lectures **via le handle du fichier**
/// (le listing du répertoire est périmé pour un fichier que le jeu garde ouvert).
pub trait LiveDir {
    /// Le dossier existe (il n'existe pas avant le chargement de la première partie).
    fn exists(&self) -> bool;
    fn has(&self, name: &str) -> bool;
    /// Taille courante, `None` si le fichier est absent/illisible.
    fn len(&self, name: &str) -> Option<u64>;
    /// Jusqu'à `max` octets à partir de `offset` (vide = rien de neuf).
    fn read_at(&self, name: &str, offset: u64, max: usize) -> std::io::Result<Vec<u8>>;
    fn read_all(&self, name: &str) -> std::io::Result<Vec<u8>>;
}

/// Échec d'un POST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostError {
    /// Réseau / 5xx : transitoire, on garde l'offset et on réessaie au tick suivant.
    Transient(String),
    /// 4xx (hors 409 sur un morceau) : rejet définitif — on arrête d'envoyer pour cette session.
    Rejected(u16, String),
}

/// Réponse acceptée à un morceau.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkReply {
    /// `200` applied / duplicate / unreadable : le serveur a les octets jusqu'à `offset + len`.
    Accepted,
    /// `409 {expected_offset}` : renvoyer depuis là.
    Gap(u64),
}

/// Le serveur storm-codex.
pub trait LiveSink {
    fn post_lobby(&mut self, bytes: &[u8]) -> Result<(), PostError>;
    fn post_chunk(
        &mut self,
        session: &str,
        stream: Stream,
        offset: u64,
        bytes: &[u8],
    ) -> Result<ChunkReply, PostError>;
    /// `404` (session plus courante côté serveur) compte comme un succès.
    fn post_end(&mut self, session: &str) -> Result<(), PostError>;
}

/// Statut affiché dans l'UI.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LiveStatus {
    /// Pas de partie en cours (dossier absent, ou partie déjà finie trouvée au démarrage).
    #[default]
    Idle,
    Streaming {
        session: String,
        tracker: u64,
        game: u64,
        /// Dernière erreur transitoire (serveur injoignable : on rattrapera).
        warning: Option<String>,
    },
    Ended {
        session: String,
    },
    Error(String),
}

impl LiveStatus {
    pub fn label(&self) -> &'static str {
        match self {
            LiveStatus::Idle => "idle",
            LiveStatus::Streaming { .. } => "streaming",
            LiveStatus::Ended { .. } => "ended",
            LiveStatus::Error(_) => "error",
        }
    }
}

/// SHA-256 hexadécimal minuscule (= `X-Session`).
pub fn session_id(lobby: &[u8]) -> String {
    hex::encode(Sha256::digest(lobby))
}

#[derive(Debug)]
struct Session {
    id: String,
    lobby: Vec<u8>,
    lobby_posted: bool,
    offsets: [u64; 2],
    warning: Option<String>,
    /// Tailles des streams au tick précédent, une fois les marqueurs de fin présents.
    end_sizes: Option<[u64; 2]>,
}

#[derive(Debug)]
enum Phase {
    /// Rien d'ouvert ; on attend un battlelobby stable.
    Idle,
    Streaming(Session),
    /// Partie finie (fermée par nous, `opened`, ou trouvée finie : pas de session ouverte).
    Done {
        lobby: String,
        opened: bool,
        sizes: [u64; 2],
    },
    /// Rejet définitif du serveur pendant la session : on n'envoie plus rien pour ce lobby.
    Halted {
        lobby: String,
        reason: String,
        sizes: [u64; 2],
    },
}

/// Machine à états du feed live. Un appel à [`tick`](Self::tick) par poll (~1 s).
#[derive(Debug)]
pub struct LiveWatcher {
    phase: Phase,
    /// Hash du battlelobby lu au tick précédent (stabilité : le jeu peut le réécrire en plusieurs
    /// passes au chargement — on n'adopte qu'un contenu identique sur deux polls).
    candidate: Option<String>,
}

impl Default for LiveWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveWatcher {
    pub fn new() -> Self {
        Self { phase: Phase::Idle, candidate: None }
    }

    /// Un poll : lit le dossier, pousse ce qui est neuf, renvoie le statut à afficher.
    pub fn tick(&mut self, dir: &impl LiveDir, sink: &mut impl LiveSink) -> LiveStatus {
        if !dir.exists() {
            self.phase = Phase::Idle;
            self.candidate = None;
            return LiveStatus::Idle;
        }
        let lobby = dir.read_all(LOBBY).ok().filter(|b| !b.is_empty());
        let hash = lobby.as_deref().map(session_id);
        let sizes = stream_sizes(dir);

        // Partie suivante (ou fichiers recréés) : on repart de zéro.
        let other_lobby = |id: &str| hash.as_deref().is_some_and(|h| h != id);
        let reset = match &self.phase {
            Phase::Idle => false,
            Phase::Streaming(s) => other_lobby(&s.id) || shrank(sizes, s.offsets),
            Phase::Done { lobby, sizes: old, .. } => {
                other_lobby(lobby) || !dir.has(DETAILS) || shrank(sizes, *old)
            }
            Phase::Halted { lobby, sizes: old, .. } => other_lobby(lobby) || shrank(sizes, *old),
        };
        if reset {
            self.phase = Phase::Idle;
            self.candidate = None;
        }

        match &self.phase {
            Phase::Idle => {
                let (Some(h), Some(bytes)) = (hash, lobby) else {
                    self.candidate = None;
                    return LiveStatus::Idle;
                };
                if self.candidate.as_ref() != Some(&h) {
                    self.candidate = Some(h);
                    return LiveStatus::Idle;
                }
                if dir.has(DETAILS) {
                    // Partie déjà finie (fichiers résiduels) : ni lobby ni session.
                    self.phase = Phase::Done { lobby: h, opened: false, sizes };
                    return LiveStatus::Idle;
                }
                self.phase = Phase::Streaming(Session {
                    id: h,
                    lobby: bytes,
                    lobby_posted: false,
                    offsets: [0, 0],
                    warning: None,
                    end_sizes: None,
                });
            }
            Phase::Done { lobby, opened, .. } => {
                return if *opened {
                    LiveStatus::Ended { session: lobby.clone() }
                } else {
                    LiveStatus::Idle
                };
            }
            Phase::Halted { reason, .. } => return LiveStatus::Error(reason.clone()),
            Phase::Streaming(_) => {}
        }

        let Phase::Streaming(s) = &mut self.phase else {
            return LiveStatus::Idle;
        };
        match pump(s, dir, sink) {
            Step::Continue => LiveStatus::Streaming {
                session: s.id.clone(),
                tracker: s.offsets[0],
                game: s.offsets[1],
                warning: s.warning.clone(),
            },
            Step::Halt(reason) => {
                let lobby = s.id.clone();
                self.phase = Phase::Halted { lobby, reason: reason.clone(), sizes };
                LiveStatus::Error(reason)
            }
            Step::Ended(sizes) => {
                let session = s.id.clone();
                self.phase = Phase::Done { lobby: session.clone(), opened: true, sizes };
                LiveStatus::Ended { session }
            }
        }
    }
}

enum Step {
    Continue,
    Halt(String),
    Ended([u64; 2]),
}

fn stream_sizes(dir: &impl LiveDir) -> [u64; 2] {
    Stream::ALL.map(|st| dir.len(st.file()).unwrap_or(0))
}

/// Un stream plus court que ce qu'on a déjà envoyé : fichiers recréés.
fn shrank(sizes: [u64; 2], offsets: [u64; 2]) -> bool {
    sizes.iter().zip(offsets).any(|(&len, off)| len < off)
}

/// Lobby (une fois), puis octets neufs des deux streams, puis fin de partie si les marqueurs sont là
/// et que les streams ne bougent plus depuis un poll (le flush final est en cours sinon).
fn pump(s: &mut Session, dir: &impl LiveDir, sink: &mut impl LiveSink) -> Step {
    if !s.lobby_posted {
        match sink.post_lobby(&s.lobby) {
            // Un lobby refusé (illisible côté serveur) n'empêche pas le feed : on n'insiste pas.
            Ok(()) | Err(PostError::Rejected(..)) => s.lobby_posted = true,
            Err(PostError::Transient(e)) => {
                s.warning = Some(e);
                return Step::Continue;
            }
        }
    }

    for stream in Stream::ALL {
        let i = stream.idx();
        for _ in 0..MAX_POSTS_PER_TICK {
            let Ok(bytes) = dir.read_at(stream.file(), s.offsets[i], MAX_CHUNK) else {
                break; // fichier pas (encore) là
            };
            if bytes.is_empty() {
                break;
            }
            match sink.post_chunk(&s.id, stream, s.offsets[i], &bytes) {
                // On avance des octets envoyés, pas de `consumed` (décodeur, s'arrête avant un
                // événement coupé) : le serveur garde lui-même la fin coupée.
                Ok(ChunkReply::Accepted) => s.offsets[i] += bytes.len() as u64,
                Ok(ChunkReply::Gap(expected)) => {
                    if expected < s.offsets[i] {
                        // Le serveur a perdu la session (redémarrage) — et peut-être son lobby
                        // courant : on le renvoie pour qu'il retrouve carte et opérateur.
                        match sink.post_lobby(&s.lobby) {
                            Ok(()) | Err(PostError::Rejected(..)) => {}
                            Err(PostError::Transient(e)) => {
                                s.warning = Some(e);
                                return Step::Continue;
                            }
                        }
                    }
                    s.offsets[i] = expected;
                }
                Err(PostError::Transient(e)) => {
                    s.warning = Some(e);
                    return Step::Continue;
                }
                Err(PostError::Rejected(code, body)) => {
                    return Step::Halt(format!("HTTP {code}: {}", body.trim()));
                }
            }
        }
    }
    s.warning = None;

    if dir.has(DETAILS) && dir.has(INIT_DATA) {
        let now = stream_sizes(dir);
        if s.end_sizes == Some(now) && s.offsets == now {
            return match sink.post_end(&s.id) {
                Ok(()) | Err(PostError::Rejected(..)) => Step::Ended(now),
                Err(PostError::Transient(e)) => {
                    s.warning = Some(e);
                    Step::Continue
                }
            };
        }
        s.end_sizes = Some(now);
    } else {
        s.end_sizes = None;
    }
    Step::Continue
}

#[cfg(test)]
mod tests;
