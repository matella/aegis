//! Feed live sans l'UI : fait tourner le watcher sur un dossier et affiche chaque changement de
//! statut. Sert à vérifier contre un storm-codex local avec un faux `TempWriteReplayP1`.
//!
//! ```text
//! cargo run -p aegis-uploader --example live_watch -- <server_url> <upload_token> [dossier]
//! ```
//! Dossier par défaut : `AEGIS_LIVE_DIR`, sinon `%TEMP%\Heroes of the Storm\TempWriteReplayP1`.
use aegis_uploader::live::io::{default_live_dir, FsLiveDir, HttpLiveSink, POLL_INTERVAL};
use aegis_uploader::live::LiveWatcher;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(url), Some(token)) = (args.get(1), args.get(2)) else {
        eprintln!("usage : live_watch <server_url> <upload_token> [dossier]");
        std::process::exit(2);
    };
    let dir = args.get(3).map(Into::into).unwrap_or_else(default_live_dir);
    println!("watching {}", dir.display());
    let fs = FsLiveDir::new(dir);
    let mut sink = HttpLiveSink::new(url.clone(), Some(token.clone()));
    let mut watcher = LiveWatcher::new();
    let mut last = None;
    loop {
        let st = watcher.tick(&fs, &mut sink);
        if last.as_ref() != Some(&st) {
            println!("{} {:?}", chrono::Local::now().format("%H:%M:%S%.3f"), st);
            last = Some(st);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}
