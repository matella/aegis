# Aegis #1 — Foundation (Shell + Dashboard + Uploader) — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the Aegis foundation — a native Windows egui app that minimizes to the system tray and runs in the background, showing a live homelab Dashboard (service health, GPU/models, now-playing) and an integrated replay Uploader, with runtime-configured box connections.

**Architecture:** New Cargo workspace `aegis` with four crates. All network access goes through **mockable traits** in `aegis-client` so dashboard logic is unit-tested without a live box. Pure logic lives in `aegis-core`/`aegis-client` (TDD'd); `aegis-uploader` is copied verbatim from `Hots-Overlay/client-rs`; `aegis-app` is a thin egui layer (verified by build + manual run). Async via tokio; shared `Arc<Mutex<AppState>>` + an event channel triggers egui repaints — the exact pattern proven in `client-rs`.

**Tech Stack:** Rust, eframe/egui 0.31, tokio, reqwest (async, rustls), tray-icon, serde/serde_json, notify (uploader), dirs. Tests with `cargo test` + mock trait impls + JSON fixtures.

**Spec:** `docs/superpowers/specs/2026-06-18-aegis-design.md` (sub-project #1 section).

**Repo location:** `/Users/matella/Documents/GitHub/aegis` (new git repo, MIT, sibling of the others).

---

## File Structure (decomposition locked here)

```
aegis/
├── Cargo.toml                         # [workspace] members
├── .gitignore                         # /target, .env
├── LICENSE                            # MIT
├── README.md
├── crates/
│   ├── aegis-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                 # re-exports
│   │       ├── config.rs              # Config: endpoints+tokens, layered load/save (TDD)
│   │       └── model.rs               # ServiceHealth, ServiceStatus, GpuState, ResidentModel,
│   │                                  #   NowPlaying, DashboardState aggregation (TDD)
│   ├── aegis-client/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                 # traits: HealthClient, GpuClient, MusicClient + mock impls
│   │       ├── parse.rs               # pure parsers: gpu json, now-playing json (TDD, fixtures)
│   │       ├── http.rs                # reqwest-backed impls of the traits
│   │       └── fixtures/              # *.json sample responses for parser tests
│   ├── aegis-uploader/                # COPIED from Hots-Overlay/client-rs (lib-ified)
│   │   ├── Cargo.toml
│   │   └── src/{lib.rs, watcher.rs, detector.rs, uploader.rs, settings.rs, state.rs}
│   └── aegis-app/
│       ├── Cargo.toml
│       ├── build.rs                   # (windows) icon; neutral, no baked secrets
│       └── src/
│           ├── main.rs                # eframe boot, tokio runtime, tray wiring
│           ├── app.rs                 # AegisApp: shared state, panel switch, repaint loop
│           ├── tray.rs                # tray icon + minimize/close-to-tray + quit
│           ├── poll.rs                # background poller: ticks client traits → AppState
│           └── views/
│               ├── dashboard.rs       # health cards, GPU, now-playing, quick actions
│               ├── uploader.rs        # uploader status + config (wraps aegis-uploader)
│               └── settings.rs        # endpoints + tokens editor, save & test
└── tests/                             # (per-crate #[cfg(test)] preferred; integration here if needed)
```

**Responsibilities:**
- `aegis-core` — no I/O. Config persistence + the data model + the *aggregation* that decides what the Dashboard shows. 100% unit-tested.
- `aegis-client` — the only crate that talks HTTP. Traits + mocks make `aegis-core`/poller testable. Parsers are pure and fixture-tested.
- `aegis-uploader` — copied, already-proven replay watcher (no rewrite).
- `aegis-app` — egui glue only; no business logic. Verified by build + manual run checklist.

---

## Chunk 1: Workspace + aegis-core (config & model, TDD)

### Task 1: Scaffold the workspace

**Files:**
- Create: `/Users/matella/Documents/GitHub/aegis/Cargo.toml`, `.gitignore`, `LICENSE`, `README.md`

- [ ] **Step 1: Create repo + workspace manifest**

```bash
mkdir -p /Users/matella/Documents/GitHub/aegis/crates && cd /Users/matella/Documents/GitHub/aegis && git init -q
```

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/aegis-core", "crates/aegis-client", "crates/aegis-uploader", "crates/aegis-app"]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time", "sync"] }
```

`.gitignore`: `/target` and `.env`. `LICENSE`: MIT (holder "Matthieu Hella", 2026). `README.md`: one paragraph + link to the spec.

- [ ] **Step 2: Commit**
```bash
git add -A && git commit -q -m "chore(aegis): workspace scaffold"
```

### Task 2: aegis-core `Config` (layered load/save)

**Files:**
- Create: `crates/aegis-core/Cargo.toml`, `crates/aegis-core/src/lib.rs`, `crates/aegis-core/src/config.rs`

**Design:** `Config { jarvis_url, storm_codex_url, orpheus_url, upload_token: Option<String> }`. Layered: **env > settings.json > built-in default** (mirror `client-rs/settings.rs`). Empty strings → treated as unset. Save writes only non-empty fields.

- [ ] **Step 1: Cargo.toml**
```toml
[package]
name = "aegis-core"
version = "0.1.0"
edition = "2021"
[dependencies]
serde.workspace = true
serde_json.workspace = true
dirs = "5"
```

- [ ] **Step 2: Write the failing test** — `crates/aegis-core/src/config.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layering_prefers_env_then_file_then_default() {
        // file provides storm_codex; env overrides jarvis; orpheus falls to default
        let file = ConfigFile { jarvis_url: None, storm_codex_url: Some("http://file:5102".into()), orpheus_url: None, upload_token: Some("tok".into()) };
        let env = |k: &str| match k { "AEGIS_JARVIS_URL" => Some("http://env:8080".into()), _ => None };
        let c = Config::resolve(file, env);
        assert_eq!(c.jarvis_url, "http://env:8080");
        assert_eq!(c.storm_codex_url, "http://file:5102");
        assert_eq!(c.orpheus_url, Config::DEFAULT_ORPHEUS);
        assert_eq!(c.upload_token.as_deref(), Some("tok"));
    }
    #[test]
    fn empty_strings_are_unset() {
        let file = ConfigFile { jarvis_url: Some("".into()), storm_codex_url: None, orpheus_url: None, upload_token: Some("".into()) };
        let c = Config::resolve(file, |_| None);
        assert_eq!(c.jarvis_url, Config::DEFAULT_JARVIS); // empty ignored → default
        assert_eq!(c.upload_token, None);                 // empty token → None
    }
}
```

- [ ] **Step 3: Run it (fails to compile — types missing)**
Run: `cargo test -p aegis-core`  → Expected: FAIL (cannot find `Config`/`ConfigFile`).

- [ ] **Step 4: Minimal implementation** — same file (above the tests):
```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    #[serde(skip_serializing_if = "Option::is_none")] pub jarvis_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub storm_codex_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub orpheus_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub upload_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub jarvis_url: String,
    pub storm_codex_url: String,
    pub orpheus_url: String,
    pub upload_token: Option<String>,
}

impl Config {
    pub const DEFAULT_JARVIS: &'static str = "http://localhost:8080";
    pub const DEFAULT_STORM: &'static str = "http://localhost:5102";
    pub const DEFAULT_ORPHEUS: &'static str = "http://localhost:3010";

    /// env > file > default. `env` is injected for testability.
    pub fn resolve(file: ConfigFile, env: impl Fn(&str) -> Option<String>) -> Config {
        let pick = |envk: &str, f: Option<String>, default: &str| {
            env(envk).or(f).filter(|s| !s.trim().is_empty()).unwrap_or_else(|| default.to_string())
        };
        let token = env("AEGIS_UPLOAD_TOKEN").or(file.upload_token).filter(|s| !s.trim().is_empty());
        Config {
            jarvis_url: pick("AEGIS_JARVIS_URL", file.jarvis_url, Self::DEFAULT_JARVIS),
            storm_codex_url: pick("AEGIS_STORM_URL", file.storm_codex_url, Self::DEFAULT_STORM),
            orpheus_url: pick("AEGIS_ORPHEUS_URL", file.orpheus_url, Self::DEFAULT_ORPHEUS),
            upload_token: token,
        }
    }

    pub fn config_path() -> PathBuf {
        let dir = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("Aegis");
        std::fs::create_dir_all(&dir).ok();
        dir.join("config.json")
    }
    pub fn load() -> Config {
        let file = std::fs::read_to_string(Self::config_path()).ok()
            .and_then(|s| serde_json::from_str::<ConfigFile>(&s).ok()).unwrap_or_default();
        Self::resolve(file, |k| std::env::var(k).ok())
    }
    pub fn save(&self) -> Result<(), String> {
        let trim = |s: &str| { let t = s.trim(); (!t.is_empty()).then(|| t.to_string()) };
        let f = ConfigFile {
            jarvis_url: trim(&self.jarvis_url), storm_codex_url: trim(&self.storm_codex_url),
            orpheus_url: trim(&self.orpheus_url), upload_token: self.upload_token.clone().filter(|s| !s.trim().is_empty()),
        };
        std::fs::write(Self::config_path(), serde_json::to_string_pretty(&f).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
}
```

- [ ] **Step 5: Run tests → PASS** — `cargo test -p aegis-core`
- [ ] **Step 6: Commit** — `git commit -am "feat(core): layered Config load/save"`

### Task 3: aegis-core `model` + dashboard aggregation (TDD)

**Files:** Create `crates/aegis-core/src/model.rs`; modify `lib.rs` to `pub mod config; pub mod model;`.

**Design:** plain data + one pure function `DashboardState::from(...)` that maps client results → what the UI renders (status per service, gpu summary, now-playing). No I/O.

- [ ] **Step 1: Failing test** — `model.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_down_when_probe_errs() {
        let h = ServiceHealth::from_probe("jarvis", Err("timeout".into()), 0);
        assert_eq!(h.status, ServiceStatus::Down);
    }
    #[test]
    fn service_up_records_latency() {
        let h = ServiceHealth::from_probe("storm-codex", Ok(()), 12);
        assert_eq!(h.status, ServiceStatus::Up);
        assert_eq!(h.latency_ms, Some(12));
    }
    #[test]
    fn gpu_summary_counts_resident() {
        let g = GpuState { models: vec![
            ResidentModel { name: "qwen".into(), resident: true, size_mb: 6600 },
            ResidentModel { name: "coder".into(), resident: false, size_mb: 0 },
        ], used_mb: 5800, total_mb: 8000 };
        assert_eq!(g.resident_count(), 1);
        assert!((g.used_fraction() - 0.725).abs() < 0.01);
    }
}
```

- [ ] **Step 2: Run → FAIL** (`cargo test -p aegis-core`).
- [ ] **Step 3: Implement** the structs + methods:
```rust
#[derive(Debug, Clone, PartialEq)] pub enum ServiceStatus { Up, Down, Unknown }
#[derive(Debug, Clone)] pub struct ServiceHealth { pub name: String, pub status: ServiceStatus, pub latency_ms: Option<u32> }
impl ServiceHealth {
    pub fn from_probe(name: &str, res: Result<(), String>, latency_ms: u32) -> Self {
        match res {
            Ok(()) => Self { name: name.into(), status: ServiceStatus::Up, latency_ms: Some(latency_ms) },
            Err(_) => Self { name: name.into(), status: ServiceStatus::Down, latency_ms: None },
        }
    }
}
#[derive(Debug, Clone, Default)] pub struct ResidentModel { pub name: String, pub resident: bool, pub size_mb: u64 }
#[derive(Debug, Clone, Default)] pub struct GpuState { pub models: Vec<ResidentModel>, pub used_mb: u64, pub total_mb: u64 }
impl GpuState {
    pub fn resident_count(&self) -> usize { self.models.iter().filter(|m| m.resident).count() }
    pub fn used_fraction(&self) -> f32 { if self.total_mb == 0 { 0.0 } else { self.used_mb as f32 / self.total_mb as f32 } }
}
#[derive(Debug, Clone, Default)] pub struct NowPlaying { pub playing: bool, pub title: Option<String>, pub artist: Option<String> }
```

- [ ] **Step 4: Run → PASS.**  **Step 5: Commit** — `feat(core): dashboard model + aggregation`.

---

## Chunk 2: aegis-client (traits, parsers, mocks, http — TDD)

### Task 4: Parser fixtures + pure parsers (TDD)

**Files:** Create `crates/aegis-client/Cargo.toml`, `src/lib.rs`, `src/parse.rs`, `src/fixtures/{gpu.json,now_playing.json,now_playing_off.json}`.

**Why:** parsing the box's real JSON is the #1 regression risk. Lock it with fixtures captured from the live box (`/api/gpu`, Orpheus `/api/playback/now`).

- [ ] **Step 1: Capture real fixtures from the box** (so tests reflect reality):
```bash
ssh matella@192.168.129.85 'curl -fsS http://localhost:5102/api/gpu' > crates/aegis-client/src/fixtures/gpu.json
ssh matella@192.168.129.85 'curl -fsS http://localhost:5102/api/now-playing' > crates/aegis-client/src/fixtures/now_playing.json
```
(If a field shape is unknown, inspect the captured JSON before writing the parser.)

- [ ] **Step 2: Failing test** — `parse.rs` (`include_str!` the fixtures):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_gpu_fixture() {
        let g = parse_gpu(include_str!("fixtures/gpu.json")).unwrap();
        assert!(g.total_mb > 0);
        assert!(g.models.iter().any(|m| m.resident)); // adjust to fixture reality
    }
    #[test]
    fn now_playing_off_is_not_playing() {
        let np = parse_now_playing(include_str!("fixtures/now_playing_off.json")).unwrap();
        assert!(!np.playing);
    }
}
```

- [ ] **Step 3: Run → FAIL.**
- [ ] **Step 4: Implement** `parse_gpu`/`parse_now_playing` (serde_json::Value, tolerant — mirror storm-codex `parseTrack`/the gpu shape). Map to `aegis_core::model` types. Adjust to the captured fixture's exact field names.
- [ ] **Step 5: Run → PASS.**  **Step 6: Commit** — `feat(client): tolerant gpu/now-playing parsers + fixtures`.

### Task 5: Client traits + mock impls (TDD)

**Files:** `crates/aegis-client/src/lib.rs`.

**Design:** async traits so the poller is testable.
```rust
#[async_trait::async_trait]
pub trait HealthClient: Send + Sync { async fn probe(&self, url: &str) -> Result<u32, String>; } // returns latency ms
#[async_trait::async_trait]
pub trait GpuClient: Send + Sync { async fn gpu(&self) -> Result<GpuState, String>; async fn free(&self) -> Result<(), String>; }
#[async_trait::async_trait]
pub trait MusicClient: Send + Sync { async fn now_playing(&self) -> Result<NowPlaying, String>; }
```
Plus `MockGpuClient { result: ... }` etc. returning canned values.

- [ ] **Step 1: Failing test** — a mock returns a GpuState; assert the consumer reads `resident_count`.
- [ ] **Step 2–5:** add `async-trait` dep, implement traits + `Mock*`, run → PASS, commit (`feat(client): mockable service traits`).

### Task 6: reqwest-backed HTTP impls

**Files:** `crates/aegis-client/src/http.rs`.

**Design:** `HttpClients { client: reqwest::Client }` implementing all three traits against config URLs. Short timeouts (3s). Health probe = `GET {url}/api/health` (storm-codex/jarvis) — Orpheus uses `/api/auth/status`; make the probe path a parameter. No unit test (network) — covered by the manual run checklist; keep the *logic* (path choice, error mapping) trivial.

- [ ] Add `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls","json"] }` + `async-trait`. Implement, `cargo build -p aegis-client`, commit (`feat(client): reqwest http impls`).

---

## Chunk 3: aegis-uploader (copy from client-rs)

### Task 7: Copy + lib-ify the uploader

**Files:** Create `crates/aegis-uploader/Cargo.toml` + copy `watcher.rs, detector.rs, uploader.rs, settings.rs, state.rs` from `/Users/matella/Documents/GitHub/Hots-Overlay/client-rs/src/`.

- [ ] **Step 1:** copy the files; create `src/lib.rs` re-exporting the modules (remove `main.rs`/egui/tray bits — keep only watcher/detector/uploader/settings/state). Copy the relevant deps (notify, ureq, tokio, serde, dirs) into Cargo.toml.
- [ ] **Step 2:** `cargo build -p aegis-uploader` → PASS (fix only compile errors from de-binary-ing; do NOT redesign).
- [ ] **Step 3:** keep/port any existing `#[cfg(test)]` from client-rs; run `cargo test -p aegis-uploader`.
- [ ] **Step 4: Commit** — `feat(uploader): copy replay watcher from client-rs (lib)`.

> Note: this is a copy by design (spec). Hots-Overlay stays untouched.

---

## Chunk 4: aegis-app shell (egui + tray + minimize-to-tray)

> egui UI isn't unit-tested. Each task's "test" is a **build + manual run checklist**. Keep all logic in core/client; the app only renders + wires.

### Task 8: eframe boot + tokio runtime + empty shell

**Files:** `crates/aegis-app/Cargo.toml`, `src/main.rs`, `src/app.rs`.

- [ ] **Step 1:** Cargo.toml deps: `eframe = "0.31"`, `egui = "0.31"`, `tokio` (workspace), `aegis-core`, `aegis-client`, `aegis-uploader`, `tray-icon`, `image`, `serde_json`.
- [ ] **Step 2:** `app.rs` — `AegisApp { state: Arc<Mutex<AppState>>, panel: Panel, rt: tokio::runtime::Handle, ... }`; `AppState { config: Config, health: Vec<ServiceHealth>, gpu: Option<GpuState>, now: NowPlaying, ... }`. `Panel { Dashboard, Uploader, Settings }`. `impl eframe::App` with a left `SidePanel` (nav buttons) + `CentralPanel` dispatch by panel. Mirror the `client-rs/app.rs` shared-state + event-channel + `ctx.request_repaint()` pattern.
- [ ] **Step 3:** `main.rs` — build tokio multi-thread runtime, load `Config`, `eframe::run_native`.
- [ ] **Step 4 (manual):** `cargo run -p aegis-app` → window opens, sidebar switches blank panels. **Commit** `feat(app): eframe shell + sidebar nav`.

### Task 9: Tray + minimize/close-to-tray + background (REQUIRED behavior)

**Files:** `src/tray.rs`, modify `src/main.rs`, `src/app.rs`.

**Behavior (from spec):** minimize **and** window-close → hide to tray (don't quit); tray click toggles show/hide; quit only via tray menu "Quit". App keeps running (hosts the uploader).

- [ ] **Step 1:** create tray icon + menu (Show/Hide, Quit) via `tray-icon` (reuse `client-rs/tray.rs` shape).
- [ ] **Step 2:** intercept the eframe close event: on `ctx.input(|i| i.viewport().close_requested())`, instead of closing, `ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false))` and cancel close. On minimize, hide to tray. Tray "Show" → `Visible(true)` + `Focus`. Tray "Quit" → real exit.
- [ ] **Step 3 (manual):** run → minimize hides to tray; close hides to tray (process still alive); tray Show restores; tray Quit exits. **Commit** `feat(app): minimize/close to tray, run in background`.

### Task 10: Settings view (endpoints + token, save & test)

**Files:** `src/views/settings.rs`, wire into `app.rs`.

- [ ] **Step 1:** form bound to `AppState.config` fields (3 URL inputs + token, password-masked). "Save & test" → `config.save()` + spawn probes via `HealthClient` for each URL → show per-service reachable/down. Mirror `client-rs` settings save + connectivity feedback.
- [ ] **Step 2 (manual):** edit URLs to the box, Save & test → all green; bad URL → red. Config persists across restart. **Commit** `feat(app): settings view (connections, save & test)`.

---

## Chunk 5: Dashboard + Uploader views + poller

### Task 11: Background poller (ticks traits → AppState)

**Files:** `src/poll.rs`, wire into `main.rs`.

- [ ] **Step 1:** a tokio task loop (every 5s, like `client-rs` connectivity check): probe each service (`HealthClient`), fetch gpu (`GpuClient`), now-playing (`MusicClient`) using `HttpClients` built from current `Config`; write results into `AppState` under the mutex; `ctx.request_repaint()`. Re-reads config each tick so Settings changes take effect live (same trick as `client-rs` connectivity loop).
- [ ] **Step 2 (manual):** with the box reachable, state populates within ~5s. **Commit** `feat(app): background poller`.

### Task 12: Dashboard view

**Files:** `src/views/dashboard.rs`.

- [ ] **Step 1:** render from `AppState` (no fetching here): **Services** card (LED + name + latency/offline per service), **GPU** card (used/total bar via `used_fraction`, resident models list, "Free GPU" button → `GpuClient::free`), **Now playing** card (title/artist or "nothing playing"), **Quick actions** (open storm-codex / Jarvis console / patches in the browser via `open` crate or `std::process` + `ctx.open_url`-equivalent → use the `open` crate). Match the mockup `2026-06-18-aegis-mockup.html` ①.
- [ ] **Step 2 (manual):** matches the mockup; LEDs reflect real box state; "Free GPU" works; quick actions open the browser. **Commit** `feat(app): dashboard view`.

### Task 13: Uploader view

**Files:** `src/views/uploader.rs`.

- [ ] **Step 1:** start the `aegis-uploader` watcher on app boot (reading server URL+token from `Config`, dirs from uploader settings); render status (watching/uploaded/pending + recent table) + folder config (add/auto-detect, like `client-rs`). Match mockup ②.
- [ ] **Step 2 (manual):** point at a replays folder → backfill + watch; uploads reach storm-codex; status updates. **Commit** `feat(app): integrated uploader view`.

---

## Chunk 6: Polish, CI, verification

### Task 14: README + windows icon + neutral build.rs

- [ ] README (run/build), `build.rs` (embed icon on windows; **no baked secrets**), app icon asset. Build clean on macOS dev + note Windows is the target. Commit.

### Task 15: CI (build) + manual acceptance checklist

- [ ] **Step 1:** `.github/workflows/ci.yml` — `cargo test --workspace` + `cargo clippy --workspace` on push.
- [ ] **Step 2:** Run the **acceptance checklist** and tick each:
  - [ ] `cargo test --workspace` green; `cargo clippy --workspace` clean.
  - [ ] App launches; sidebar switches Dashboard/Uploader/Settings.
  - [ ] Minimize → tray; close → tray (process alive); tray Show restores; tray Quit exits.
  - [ ] Settings → point at box → Save & test → all services green; persists across restart.
  - [ ] Dashboard: real health LEDs, GPU bar + models from `/api/gpu`, Free GPU works, now-playing shows current Spotify track, quick actions open browser.
  - [ ] Uploader: watches a folder, uploads to storm-codex, status updates, resumes after server blip.
- [ ] **Step 3:** Commit + (ask before pushing / `gh repo create`).

---

## Out of scope (this plan)
Jarvis chat + action pipeline (#2), voice (#3), satellite action catalog (#4), signed MSI packaging. Those are separate specs/plans.

## Notes for the implementer
- **TDD discipline:** core + client parsers/aggregation are red→green→commit. egui views are build+manual-run (logic lives in core/client, kept thin in the view).
- **Mock-first:** never reach the network in `aegis-core` tests; use `aegis-client` mock traits.
- **Reuse, don't redesign:** `aegis-uploader` is a copy; only fix compile errors from removing the binary/UI bits.
- **Capture real fixtures** (Task 4) from the live box so parsers match reality.
