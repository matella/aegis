//! Tests de la machine à états : dossier en mémoire + serveur simulé (même règle de morceaux que
//! storm-codex `plan_chunk` : offset > reçu → 409 ; sinon accepté, chevauchement absorbé).
use super::*;
use std::collections::HashMap;

#[derive(Default)]
struct FakeDir {
    exists: bool,
    files: HashMap<String, Vec<u8>>,
}

impl FakeDir {
    fn new() -> Self {
        Self { exists: true, files: HashMap::new() }
    }
    fn put(&mut self, name: &str, bytes: &[u8]) {
        self.files.insert(name.into(), bytes.to_vec());
    }
    fn append(&mut self, name: &str, bytes: &[u8]) {
        self.files.entry(name.into()).or_default().extend_from_slice(bytes);
    }
    fn remove(&mut self, name: &str) {
        self.files.remove(name);
    }
    /// Un dossier de partie qui vient de se créer : lobby + streams vides.
    fn game(lobby: &[u8]) -> Self {
        let mut d = Self::new();
        d.put(LOBBY, lobby);
        d.put(Stream::Tracker.file(), b"");
        d.put(Stream::Game.file(), b"");
        d
    }
    fn finish(&mut self) {
        self.put(DETAILS, b"details");
        self.put(INIT_DATA, b"init");
    }
}

impl LiveDir for FakeDir {
    fn exists(&self) -> bool {
        self.exists
    }
    fn has(&self, name: &str) -> bool {
        self.exists && self.files.contains_key(name)
    }
    fn len(&self, name: &str) -> Option<u64> {
        self.files.get(name).filter(|_| self.exists).map(|b| b.len() as u64)
    }
    fn read_at(&self, name: &str, offset: u64, max: usize) -> std::io::Result<Vec<u8>> {
        let b = self
            .files
            .get(name)
            .filter(|_| self.exists)
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        let start = (offset as usize).min(b.len());
        let end = (start + max).min(b.len());
        Ok(b[start..end].to_vec())
    }
    fn read_all(&self, name: &str) -> std::io::Result<Vec<u8>> {
        self.read_at(name, 0, usize::MAX / 2)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Lobby(Vec<u8>),
    Chunk { session: String, stream: Stream, offset: u64, len: usize },
    End(String),
}

#[derive(Default)]
struct FakeSink {
    calls: Vec<Call>,
    down: bool,
    reject_chunks: Option<u16>,
    end_404: bool,
    session: String,
    received: [u64; 2],
    data: [Vec<u8>; 2],
}

impl FakeSink {
    /// Redémarrage serveur : la session en mémoire est perdue.
    fn restart(&mut self) {
        self.session.clear();
        self.received = [0, 0];
        self.data = [Vec::new(), Vec::new()];
    }
    fn chunks(&self) -> Vec<(Stream, u64, usize)> {
        self.calls
            .iter()
            .filter_map(|c| match c {
                Call::Chunk { stream, offset, len, .. } => Some((*stream, *offset, *len)),
                _ => None,
            })
            .collect()
    }
    fn ends(&self) -> usize {
        self.calls.iter().filter(|c| matches!(c, Call::End(_))).count()
    }
    fn lobbies(&self) -> usize {
        self.calls.iter().filter(|c| matches!(c, Call::Lobby(_))).count()
    }
}

impl LiveSink for FakeSink {
    fn post_lobby(&mut self, bytes: &[u8]) -> Result<(), PostError> {
        if self.down {
            return Err(PostError::Transient("connection refused".into()));
        }
        self.calls.push(Call::Lobby(bytes.to_vec()));
        Ok(())
    }
    fn post_chunk(
        &mut self,
        session: &str,
        stream: Stream,
        offset: u64,
        bytes: &[u8],
    ) -> Result<ChunkReply, PostError> {
        if self.down {
            return Err(PostError::Transient("connection refused".into()));
        }
        self.calls.push(Call::Chunk {
            session: session.into(),
            stream,
            offset,
            len: bytes.len(),
        });
        if let Some(code) = self.reject_chunks {
            return Err(PostError::Rejected(code, "nope".into()));
        }
        if self.session != session {
            // Un id nouveau remplace la session (contrat serveur).
            self.restart();
            self.session = session.into();
        }
        let i = stream.idx();
        let received = self.received[i];
        if offset > received {
            return Ok(ChunkReply::Gap(received));
        }
        let end = offset + bytes.len() as u64;
        if end > received {
            let skip = (received - offset) as usize;
            self.data[i].extend_from_slice(&bytes[skip..]);
            self.received[i] = end;
        }
        Ok(ChunkReply::Accepted)
    }
    fn post_end(&mut self, session: &str) -> Result<(), PostError> {
        if self.down {
            return Err(PostError::Transient("connection refused".into()));
        }
        self.calls.push(Call::End(session.into()));
        let _ = self.end_404; // 404 = succès côté client : rien à simuler de plus
        Ok(())
    }
}

fn block(n: u8) -> Vec<u8> {
    vec![n; 4096]
}

/// Deux ticks : le premier voit le lobby, le second le confirme stable et ouvre la session.
fn open(w: &mut LiveWatcher, d: &FakeDir, s: &mut FakeSink) -> LiveStatus {
    w.tick(d, s);
    w.tick(d, s)
}

#[test]
fn session_id_is_lowercase_hex_sha256() {
    assert_eq!(
        session_id(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn no_folder_is_idle_and_silent() {
    let d = FakeDir { exists: false, ..Default::default() };
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    assert_eq!(w.tick(&d, &mut s), LiveStatus::Idle);
    assert_eq!(w.tick(&d, &mut s), LiveStatus::Idle);
    assert!(s.calls.is_empty());
}

#[test]
fn lobby_posted_once_after_it_is_stable() {
    let d = FakeDir::game(b"LOBBY-1");
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    // 1er tick : lobby vu, pas encore adopté (stabilité).
    assert_eq!(w.tick(&d, &mut s), LiveStatus::Idle);
    assert_eq!(s.lobbies(), 0);
    let st = w.tick(&d, &mut s);
    assert_eq!(st.label(), "streaming");
    assert_eq!(s.calls, vec![Call::Lobby(b"LOBBY-1".to_vec())]);
    for _ in 0..5 {
        w.tick(&d, &mut s);
    }
    assert_eq!(s.lobbies(), 1, "dédup par contenu : pas de repost");
}

#[test]
fn lobby_rewritten_during_loading_only_final_content_is_adopted() {
    let mut d = FakeDir::game(b"PASS-1");
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    w.tick(&d, &mut s);
    d.put(LOBBY, b"PASS-2");
    w.tick(&d, &mut s);
    assert_eq!(s.lobbies(), 0);
    w.tick(&d, &mut s);
    assert_eq!(s.calls, vec![Call::Lobby(b"PASS-2".to_vec())]);
}

#[test]
fn streams_post_only_appended_bytes_with_offsets_and_session() {
    let mut d = FakeDir::game(b"LOBBY");
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    open(&mut w, &d, &mut s);
    assert!(s.chunks().is_empty(), "streams vides : rien à envoyer");

    d.append(Stream::Tracker.file(), &[block(1), block(2)].concat());
    w.tick(&d, &mut s);
    d.append(Stream::Tracker.file(), &block(3));
    d.append(Stream::Game.file(), &block(9));
    let st = w.tick(&d, &mut s);

    assert_eq!(
        s.chunks(),
        vec![
            (Stream::Tracker, 0, 8192),
            (Stream::Tracker, 8192, 4096),
            (Stream::Game, 0, 4096),
        ]
    );
    let sid = session_id(b"LOBBY");
    assert!(s.calls.iter().all(|c| match c {
        Call::Chunk { session, .. } => session == &sid,
        _ => true,
    }));
    assert_eq!(
        st,
        LiveStatus::Streaming { session: sid, tracker: 12288, game: 4096, warning: None }
    );
    assert_eq!(s.data[0], [block(1), block(2), block(3)].concat());
}

#[test]
fn lobby_is_posted_before_first_chunk() {
    let mut d = FakeDir::game(b"LOBBY");
    d.append(Stream::Tracker.file(), &block(1));
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    open(&mut w, &d, &mut s);
    assert!(matches!(s.calls[0], Call::Lobby(_)));
    assert!(matches!(s.calls[1], Call::Chunk { .. }));
}

#[test]
fn large_backlog_is_split_into_bounded_posts() {
    let mut d = FakeDir::game(b"LOBBY");
    d.append(Stream::Tracker.file(), &vec![7u8; MAX_CHUNK * 2 + 100]);
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    open(&mut w, &d, &mut s);
    assert_eq!(
        s.chunks(),
        vec![
            (Stream::Tracker, 0, MAX_CHUNK),
            (Stream::Tracker, MAX_CHUNK as u64, MAX_CHUNK),
            (Stream::Tracker, 2 * MAX_CHUNK as u64, 100),
        ]
    );
}

#[test]
fn server_unreachable_keeps_offset_and_catches_up() {
    let mut d = FakeDir::game(b"LOBBY");
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    open(&mut w, &d, &mut s);
    d.append(Stream::Tracker.file(), &block(1));
    w.tick(&d, &mut s);

    s.down = true;
    d.append(Stream::Tracker.file(), &block(2));
    let st = w.tick(&d, &mut s);
    match st {
        LiveStatus::Streaming { tracker, warning, .. } => {
            assert_eq!(tracker, 4096, "offset gardé");
            assert!(warning.is_some());
        }
        other => panic!("{other:?}"),
    }
    d.append(Stream::Tracker.file(), &block(3));
    s.down = false;
    w.tick(&d, &mut s);
    assert_eq!(s.chunks().last(), Some(&(Stream::Tracker, 4096, 8192)));
    assert_eq!(s.data[0], [block(1), block(2), block(3)].concat());
}

#[test]
fn gap_409_rewinds_to_expected_offset() {
    let mut d = FakeDir::game(b"LOBBY");
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    open(&mut w, &d, &mut s);
    d.append(Stream::Tracker.file(), &[block(1), block(2)].concat());
    w.tick(&d, &mut s);

    s.restart(); // le serveur a perdu la session → 409 expected 0
    d.append(Stream::Tracker.file(), &block(3));
    let st = w.tick(&d, &mut s);
    assert_eq!(
        s.chunks()[1..],
        [(Stream::Tracker, 8192, 4096), (Stream::Tracker, 0, 12288)]
    );
    assert_eq!(s.data[0], [block(1), block(2), block(3)].concat());
    assert!(matches!(st, LiveStatus::Streaming { tracker: 12288, .. }));
}

#[test]
fn rewind_reposts_lobby_before_resending() {
    // Serveur redémarré : il a perdu la session, et peut-être son lobby courant.
    let mut d = FakeDir::game(b"LOBBY");
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    open(&mut w, &d, &mut s);
    d.append(Stream::Tracker.file(), &block(1));
    w.tick(&d, &mut s);
    s.restart();
    d.append(Stream::Tracker.file(), &block(2));
    w.tick(&d, &mut s);
    let tail: Vec<_> = s.calls[2..].to_vec();
    assert!(matches!(tail[0], Call::Chunk { offset: 4096, .. }), "{tail:?}");
    assert_eq!(tail[1], Call::Lobby(b"LOBBY".to_vec()));
    assert!(matches!(tail[2], Call::Chunk { offset: 0, len: 8192, .. }), "{tail:?}");
}

#[test]
fn rejection_stops_sending_and_reports_error() {
    let mut d = FakeDir::game(b"LOBBY");
    d.append(Stream::Tracker.file(), &block(1));
    let mut s = FakeSink { reject_chunks: Some(401), ..Default::default() };
    let mut w = LiveWatcher::new();
    let st = open(&mut w, &d, &mut s);
    assert_eq!(st.label(), "error");
    let n = s.chunks().len();
    d.append(Stream::Tracker.file(), &block(2));
    for _ in 0..3 {
        assert_eq!(w.tick(&d, &mut s).label(), "error");
    }
    assert_eq!(s.chunks().len(), n, "plus aucun envoi après un rejet");
}

#[test]
fn end_of_game_flushes_then_posts_end_once() {
    let mut d = FakeDir::game(b"LOBBY");
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    open(&mut w, &d, &mut s);
    d.append(Stream::Tracker.file(), &block(1));
    w.tick(&d, &mut s);

    // Flush final : bloc partiel + details/initData en une fois.
    d.append(Stream::Tracker.file(), &[5u8; 1000]);
    d.append(Stream::Game.file(), &[6u8; 300]);
    d.finish();
    let mut ended = None;
    for _ in 0..4 {
        let st = w.tick(&d, &mut s);
        if st.label() == "ended" {
            ended = Some(st);
            break;
        }
    }
    assert_eq!(ended, Some(LiveStatus::Ended { session: session_id(b"LOBBY") }));
    assert_eq!(s.ends(), 1);
    let end_pos = s.calls.iter().position(|c| matches!(c, Call::End(_))).unwrap();
    assert!(
        s.calls[..end_pos].contains(&Call::Chunk {
            session: session_id(b"LOBBY"),
            stream: Stream::Game,
            offset: 0,
            len: 300
        }),
        "le flush part avant /end"
    );
    assert_eq!(s.received, [5096, 300]);
    for _ in 0..3 {
        assert_eq!(w.tick(&d, &mut s).label(), "ended");
    }
    assert_eq!(s.ends(), 1);
}

#[test]
fn end_waits_until_streams_stop_growing() {
    let mut d = FakeDir::game(b"LOBBY");
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    open(&mut w, &d, &mut s);
    d.finish();
    d.append(Stream::Tracker.file(), &block(1));
    w.tick(&d, &mut s);
    d.append(Stream::Tracker.file(), &[1u8; 10]); // le flush n'était pas fini
    w.tick(&d, &mut s);
    assert_eq!(s.ends(), 0);
    w.tick(&d, &mut s);
    assert_eq!(s.ends(), 1);
    assert_eq!(s.received[0], 4106);
}

#[test]
fn end_retried_when_server_down() {
    let mut d = FakeDir::game(b"LOBBY");
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    open(&mut w, &d, &mut s);
    d.finish();
    s.down = true;
    for _ in 0..3 {
        assert_eq!(w.tick(&d, &mut s).label(), "streaming");
    }
    s.down = false;
    w.tick(&d, &mut s);
    assert_eq!(s.ends(), 1);
}

#[test]
fn finished_game_at_startup_opens_nothing() {
    let mut d = FakeDir::game(b"OLD");
    d.append(Stream::Tracker.file(), &block(1));
    d.finish();
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    for _ in 0..4 {
        assert_eq!(w.tick(&d, &mut s), LiveStatus::Idle);
    }
    assert!(s.calls.is_empty(), "ni lobby ni morceau ni fin pour une partie déjà finie");
}

#[test]
fn next_game_after_end_opens_a_fresh_session() {
    let mut d = FakeDir::game(b"G1");
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    open(&mut w, &d, &mut s);
    d.append(Stream::Tracker.file(), &block(1));
    d.finish();
    for _ in 0..3 {
        w.tick(&d, &mut s);
    }
    assert_eq!(s.ends(), 1);

    // Partie suivante : le jeu recrée les fichiers.
    let mut d = FakeDir::game(b"G2");
    d.append(Stream::Tracker.file(), &block(2));
    open(&mut w, &d, &mut s);
    let st = w.tick(&d, &mut s);
    assert_eq!(
        st,
        LiveStatus::Streaming {
            session: session_id(b"G2"),
            tracker: 4096,
            game: 0,
            warning: None
        }
    );
    assert_eq!(s.calls.iter().filter(|c| **c == Call::Lobby(b"G2".to_vec())).count(), 1);
    assert_eq!(s.chunks().last(), Some(&(Stream::Tracker, 0, 4096)));
}

#[test]
fn details_disappearing_reopens_for_same_lobby_bytes() {
    // Le lobby survit entre deux parties identiques ? On rouvre dès que details disparaît.
    let mut d = FakeDir::game(b"SAME");
    d.finish();
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    open(&mut w, &d, &mut s);
    assert!(s.calls.is_empty());
    d.remove(DETAILS);
    d.remove(INIT_DATA);
    d.put(Stream::Tracker.file(), &block(4));
    open(&mut w, &d, &mut s);
    assert_eq!(s.lobbies(), 1);
    assert_eq!(s.chunks(), vec![(Stream::Tracker, 0, 4096)]);
}

#[test]
fn shrinking_stream_resets_the_session() {
    let mut d = FakeDir::game(b"LOBBY");
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    open(&mut w, &d, &mut s);
    d.append(Stream::Tracker.file(), &[block(1), block(2)].concat());
    w.tick(&d, &mut s);
    // Fichier recréé (reconnexion ?) : repart de zéro.
    d.put(Stream::Tracker.file(), &block(3));
    open(&mut w, &d, &mut s);
    w.tick(&d, &mut s);
    assert_eq!(s.chunks().last(), Some(&(Stream::Tracker, 0, 4096)));
}

#[test]
fn folder_vanishing_returns_to_idle() {
    let mut d = FakeDir::game(b"LOBBY");
    let mut s = FakeSink::default();
    let mut w = LiveWatcher::new();
    open(&mut w, &d, &mut s);
    d.exists = false;
    assert_eq!(w.tick(&d, &mut s), LiveStatus::Idle);
}
