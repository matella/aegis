//! Parseurs PURS des réponses du box → types aegis-core. Tolérants (champs manquants) et testés sur
//! des fixtures capturées en réel. C'est le point de régression n°1 → verrouillé par les tests.
use aegis_core::{GpuState, NowPlaying, ResidentModel, TorrentItem, Torrents};
use serde_json::Value;

/// `/api/torrents` du gateway (snapshot qBittorrent : `{downloading, seeding, dl_speed, up_speed,
/// items:[{name, progress, dl_speed}]}`). Tolérant aux champs manquants.
pub fn parse_torrents(body: &str) -> Result<Torrents, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let u = |x: Option<&Value>| x.and_then(Value::as_u64).unwrap_or(0);
    let items = v
        .get("items")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|t| TorrentItem {
                    name: t.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                    progress: t.get("progress").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                    dl_speed: t.get("dl_speed").and_then(Value::as_u64).unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Torrents {
        downloading: u(v.get("downloading")) as u32,
        seeding: u(v.get("seeding")) as u32,
        dl_speed: u(v.get("dl_speed")),
        up_speed: u(v.get("up_speed")),
        items,
    })
}

/// `/api/gpu` de la gateway Jarvis → `GpuState`. Forme :
/// `{reachable, resident:[{model,vram_gib,size_gib,processor,expires_in_s}], model_count, vram_held_gib}`.
pub fn parse_gpu(body: &str) -> Result<GpuState, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let f = |x: Option<&Value>| x.and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let resident = v
        .get("resident")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|m| ResidentModel {
                    model: m.get("model").and_then(Value::as_str).unwrap_or("?").to_string(),
                    vram_gib: f(m.get("vram_gib")),
                    size_gib: f(m.get("size_gib")),
                    processor: m.get("processor").and_then(Value::as_str).unwrap_or("GPU").to_string(),
                    expires_in_s: m.get("expires_in_s").and_then(Value::as_u64).map(|n| n as u32),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(GpuState {
        reachable: v.get("reachable").and_then(Value::as_bool).unwrap_or(true),
        vram_held_gib: f(v.get("vram_held_gib")),
        resident,
    })
}

/// Lecture en cours → `NowPlaying`. Tolère les DEUX formes :
///  - Orpheus brut : `{isPlaying, track:{name, artists:[{name}]}}`
///  - storm-codex enveloppé : `{authenticated, current:{isPlaying, track:{…}}}`.
pub fn parse_now_playing(body: &str) -> Result<NowPlaying, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    // déballe la forme enveloppée si présente
    let cur = v.get("current").filter(|c| !c.is_null()).unwrap_or(&v);
    let authed = v.get("authenticated").and_then(Value::as_bool).unwrap_or(true);
    let is_playing = cur.get("isPlaying").and_then(Value::as_bool).unwrap_or(false);
    let track = cur.get("track");
    let title = track
        .and_then(|t| t.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let artist = track
        .and_then(|t| t.get("artists"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.get("name").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|s| !s.is_empty());
    // Pochette : `track.album.images` (Spotify, du plus grand au plus petit). La vignette fait
    // ~46px → la plus petite image (dernière) suffit et économise la bande passante.
    let cover_url = track
        .and_then(|t| t.get("album"))
        .and_then(|al| al.get("images"))
        .and_then(Value::as_array)
        .and_then(|imgs| {
            imgs.iter().rev().find_map(|im| im.get("url").and_then(Value::as_str)).map(str::to_string)
        });
    let progress_ms = cur.get("progressMs").and_then(Value::as_u64).map(|n| n as u32);
    let duration_ms = track.and_then(|t| t.get("durationMs")).and_then(Value::as_u64).map(|n| n as u32);
    Ok(NowPlaying {
        playing: authed && is_playing && title.is_some(),
        title,
        artist,
        cover_url,
        progress_ms,
        duration_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gpu_fixture() {
        let g = parse_gpu(include_str!("fixtures/gpu.json")).unwrap();
        assert!(g.reachable);
        assert_eq!(g.model_count(), 1);
        assert!((g.vram_held_gib - 6.6).abs() < 0.01);
        assert_eq!(g.resident[0].processor, "GPU");
        assert_eq!(g.resident[0].expires_in_s, Some(287));
    }

    #[test]
    fn parses_now_playing_orpheus_shape() {
        let np = parse_now_playing(include_str!("fixtures/now_playing.json")).unwrap();
        assert!(np.playing);
        assert_eq!(np.title.as_deref(), Some("California Gurls"));
        assert_eq!(np.artist.as_deref(), Some("Katy Perry, Snoop Dogg"));
        // Pochette : on retient la plus petite image (dernière du tableau).
        assert_eq!(np.cover_url.as_deref(), Some("https://i.scdn.co/image/small"));
        assert_eq!(np.progress_ms, Some(1482));
        assert_eq!(np.duration_ms, Some(235000));
    }

    #[test]
    fn now_playing_off_is_not_playing() {
        let np = parse_now_playing(include_str!("fixtures/now_playing_off.json")).unwrap();
        assert!(!np.playing);
        assert!(np.title.is_none());
    }

    #[test]
    fn parses_storm_codex_wrapped_shape() {
        let body = r#"{"authenticated":true,"current":{"isPlaying":true,"track":{"name":"X","artists":[{"name":"Y"}]}}}"#;
        let np = parse_now_playing(body).unwrap();
        assert!(np.playing);
        assert_eq!(np.title.as_deref(), Some("X"));
    }

    #[test]
    fn parses_torrents_snapshot() {
        let body = r#"{"total":5,"downloading":2,"seeding":3,"dl_speed":1048576,"up_speed":2048,
            "items":[{"name":"Foo","state":"downloading","progress":0.42,"dl_speed":1048576}]}"#;
        let t = parse_torrents(body).unwrap();
        assert_eq!(t.downloading, 2);
        assert_eq!(t.seeding, 3);
        assert_eq!(t.dl_speed, 1048576);
        assert_eq!(t.items.len(), 1);
        assert!((t.items[0].progress - 0.42).abs() < 0.001);
    }

    #[test]
    fn unauthenticated_is_not_playing() {
        let body = r#"{"authenticated":false}"#;
        assert!(!parse_now_playing(body).unwrap().playing);
    }
}
