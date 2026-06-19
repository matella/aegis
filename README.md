# Aegis

Native Windows (Rust / egui) **operator console** for [Jarvis](https://github.com/matella/Jarvis)
and its satellites (storm-codex, Orpheus, the replay uploader, the box). Aegis is a Jarvis *client*
+ a local executor for Jarvis's desktop actions — it shows live homelab status, hosts the replay
uploader, and minimizes to the system tray to run in the background.

Design, plan & mockups live in [`docs/`](docs/): [`design.md`](docs/design.md) (umbrella spec),
[`plan-1-foundation.md`](docs/plan-1-foundation.md), and [`aegis-mockup.html`](docs/aegis-mockup.html)
(open in a browser — visual intent, not the real render).

Built in phases: **#1 Foundation** (shell + dashboard + uploader) → #2 Jarvis chat + actions →
#3 voice ("hey Jarvis") → #4 satellite action catalog. All logic is TDD'd behind mockable traits.

## Packaging (Windows MSI)

The installer is built with [`cargo-wix`](https://github.com/volks73/cargo-wix) (WiX Toolset v3).
The WiX source lives in [`crates/aegis-app/wix/main.wxs`](crates/aegis-app/wix/main.wxs).

```powershell
# one-time tooling (the WiX install needs admin rights):
winget install WiXToolset.WiXToolset      # WiX Toolset v3.14 (candle/light)
cargo install cargo-wix

# build the MSI → target/wix/Aegis-<version>-x86_64.msi
pwsh -File package.ps1
```

`package.ps1` locates WiX, builds the release binary, and runs `cargo wix` from the package
directory (needed in a workspace so the wxs's package-relative paths resolve).

The MSI is **unsigned** — Windows SmartScreen warns on first run ("More info → Run anyway").
Signing would need a code-signing certificate (none configured).
