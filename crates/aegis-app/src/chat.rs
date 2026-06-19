//! Client chat Jarvis via la gateway `/ws` (mode ouvert → pas de token). On envoie `{"text": …}`,
//! on reçoit `{kind:"ready"}`, `{kind:"history", messages}`, `{kind:"turn", result:{message}}`.
//! Reconnexion automatique. La logique d'actions (Intents `desktop.*`) viendra en #2b.
use crate::audio::Player;
use aegis_core::Config;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

/// Message sortant vers la gateway : texte (chat) ou frame audio 16 kHz (voix « hey Jarvis »).
pub enum Outbound {
    Text(String),
    Wake(String), // PCM16 16 kHz en base64
}

/// Lien d'action attaché à un message (artifact `kind:"link"` de Jarvis), résolu en URL absolue.
pub struct ChatLink {
    pub title: String,
    pub url: String,
}

#[derive(Default)]
pub struct ChatMsg {
    pub role: String, // "you" | "jarvis"
    pub text: String,
    pub links: Vec<ChatLink>,
    /// Action gated proposée par Jarvis (`route:"propose"`) → à confirmer. `intent_id` = l'intent
    /// en attente côté gateway ; on confirme/annule en renvoyant « confirm »/« reject » sur le /ws.
    pub proposal: bool,
    #[allow(dead_code)] // tracé pour debug / futur appel REST ; la confirmation passe par le chat
    pub intent_id: Option<String>,
}

#[derive(Default)]
pub struct ChatState {
    pub messages: Vec<ChatMsg>,
    pub connected: bool,
    pub thinking: bool,
    pub wake: bool, // « hey Jarvis » détecté → écoute en cours (indicateur UI)
}

/// Convertit l'URL HTTP de la gateway en URL WebSocket `/ws`.
fn to_ws(url: &str) -> String {
    let base = url.trim_end_matches('/');
    let ws = if let Some(r) = base.strip_prefix("https://") {
        format!("wss://{r}")
    } else if let Some(r) = base.strip_prefix("http://") {
        format!("ws://{r}")
    } else {
        format!("ws://{base}")
    };
    format!("{ws}/ws")
}

/// Résout un artifact lien (`{app, path}` ou `{url}`) en URL absolue via la config Aegis.
fn resolve_url(cfg: &Config, app: &str, path: &str, url: Option<&str>) -> Option<String> {
    if let Some(u) = url.filter(|u| !u.is_empty()) {
        return Some(u.to_string());
    }
    let base = match app {
        "storm-codex" | "storm_codex" | "stormcodex" => &cfg.storm_codex_url,
        "orpheus" => &cfg.orpheus_url,
        "jarvis" => &cfg.jarvis_url,
        _ => return None,
    };
    Some(format!("{}{}", base.trim_end_matches('/'), path))
}

fn handle_frame(
    text: &str,
    state: &Arc<Mutex<ChatState>>,
    ctx: &egui::Context,
    cfg: &Arc<Mutex<Config>>,
    player: &Player,
) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return };
    match v.get("kind").and_then(|k| k.as_str()) {
        // Voix : wake-word détecté → accusé « il t'a entendu » (chime aigu + phrase parlée).
        Some("wake") => {
            state.lock().unwrap().wake = true;
            let reply = cfg.lock().unwrap().wake_reply.clone();
            player.chime(880.0);
            crate::audio::speak(&reply);
        }
        // Fin de parole détectée par le serveur → transcription + accusé « il réfléchit » (chime
        // grave + phrase). La vraie réponse suit dans `turn`.
        Some("transcript") => {
            if let Some(t) = v.get("text").and_then(|t| t.as_str()).filter(|t| !t.is_empty()) {
                {
                    let mut s = state.lock().unwrap();
                    s.messages.push(ChatMsg { role: "you".into(), text: t.to_string(), ..Default::default() });
                    s.thinking = true;
                    s.wake = false;
                }
                let reply = cfg.lock().unwrap().thinking_reply.clone();
                player.chime(440.0);
                crate::audio::speak(&reply);
            }
        }
        Some("tts") => {
            if let Some(wav) = v.get("wav").and_then(|w| w.as_str()) {
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(wav) {
                    player.play(bytes);
                }
            }
        }
        Some("turn") => {
            let result = v.get("result");
            let msg = result
                .and_then(|r| r.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            // Action gated proposée (`route:"propose"` + intent_id) → à confirmer dans l'UI.
            let route = result.and_then(|r| r.get("route")).and_then(|x| x.as_str()).unwrap_or("");
            let intent_id =
                result.and_then(|r| r.get("intent_id")).and_then(|x| x.as_str()).map(str::to_string);
            let proposal = route == "propose" && intent_id.is_some();
            // Actions : artifacts kind:"link" → résolus en URL ; `open:true` = ouverture auto.
            let mut links = Vec::new();
            let mut to_open = Vec::new();
            if let Some(arts) = result.and_then(|r| r.get("artifacts")).and_then(|a| a.as_array()) {
                let cfg = cfg.lock().unwrap();
                for a in arts {
                    if a.get("kind").and_then(|k| k.as_str()) != Some("link") {
                        continue;
                    }
                    let title = a.get("title").and_then(|t| t.as_str()).unwrap_or("Open").to_string();
                    let d = a.get("data");
                    let app = d.and_then(|d| d.get("app")).and_then(|x| x.as_str()).unwrap_or("");
                    let path = d.and_then(|d| d.get("path")).and_then(|x| x.as_str()).unwrap_or("");
                    let url_field = d.and_then(|d| d.get("url")).and_then(|x| x.as_str());
                    let open = d.and_then(|d| d.get("open")).and_then(|x| x.as_bool()).unwrap_or(false);
                    if let Some(url) = resolve_url(&cfg, app, path, url_field) {
                        if open {
                            to_open.push(url.clone());
                        }
                        links.push(ChatLink { title, url });
                    }
                }
            }
            {
                let mut s = state.lock().unwrap();
                if !msg.is_empty() || !links.is_empty() {
                    s.messages.push(ChatMsg { role: "jarvis".into(), text: msg, links, proposal, intent_id });
                }
                s.thinking = false;
            }
            for u in to_open {
                let _ = open::that(&u); // ouverture déterministe côté desktop (non destructif)
            }
        }
        Some("history") => {
            if let Some(arr) = v.get("messages").and_then(|m| m.as_array()) {
                let mut s = state.lock().unwrap();
                s.messages = arr
                    .iter()
                    .filter_map(|m| {
                        let role = m.get("role")?.as_str()?.to_string();
                        let text = m.get("content")?.as_str()?.to_string();
                        (!text.is_empty()).then_some(ChatMsg { role, text, ..Default::default() })
                    })
                    .collect();
            }
        }
        _ => {} // ready / presence : ignorés
    }
    ctx.request_repaint();
}

/// Démarre le client chat (tâche tokio, reconnexion auto). Renvoie l'émetteur pour envoyer du texte.
pub fn spawn(
    rt: &tokio::runtime::Handle,
    gateway_url: String,
    cfg: Arc<Mutex<Config>>,
    state: Arc<Mutex<ChatState>>,
    ctx: egui::Context,
    player: Player,
) -> tokio::sync::mpsc::UnboundedSender<Outbound> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Outbound>();
    rt.spawn(async move {
        loop {
            let url = to_ws(&gateway_url);
            match tokio_tungstenite::connect_async(&url).await {
                Ok((ws, _)) => {
                    {
                        let mut s = state.lock().unwrap();
                        s.connected = true;
                    }
                    ctx.request_repaint();
                    let (mut write, mut read) = ws.split();
                    loop {
                        tokio::select! {
                            out = rx.recv() => match out {
                                Some(outbound) => {
                                    let frame = match outbound {
                                        Outbound::Text(text) => serde_json::json!({ "text": text }),
                                        Outbound::Wake(pcm) => serde_json::json!({ "kind": "wake_chunk", "pcm": pcm }),
                                    }
                                    .to_string();
                                    if write.send(Message::Text(frame)).await.is_err() {
                                        break;
                                    }
                                }
                                None => return, // émetteur droppé → app fermée
                            },
                            inc = read.next() => match inc {
                                Some(Ok(Message::Text(t))) => handle_frame(&t, &state, &ctx, &cfg, &player),
                                Some(Ok(_)) => {} // binaire/ping ignorés
                                _ => break,       // déconnexion / erreur
                            },
                        }
                    }
                    {
                        let mut s = state.lock().unwrap();
                        s.connected = false;
                    }
                    ctx.request_repaint();
                }
                Err(_) => {
                    state.lock().unwrap().connected = false;
                    ctx.request_repaint();
                }
            }
            tokio::time::sleep(Duration::from_secs(3)).await; // retry
        }
    });
    tx
}
