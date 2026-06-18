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
