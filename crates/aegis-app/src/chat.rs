//! Client chat Jarvis via la gateway `/ws` (mode ouvert → pas de token). On envoie `{"text": …}`,
//! on reçoit `{kind:"ready"}`, `{kind:"history", messages}`, `{kind:"turn", result:{message}}`.
//! Reconnexion automatique. La logique d'actions (Intents `desktop.*`) viendra en #2b.
use futures_util::{SinkExt, StreamExt};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

pub struct ChatMsg {
    pub role: String, // "you" | "jarvis"
    pub text: String,
}

#[derive(Default)]
pub struct ChatState {
    pub messages: Vec<ChatMsg>,
    pub connected: bool,
    pub thinking: bool,
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

fn handle_frame(text: &str, state: &Arc<Mutex<ChatState>>, ctx: &egui::Context) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return };
    match v.get("kind").and_then(|k| k.as_str()) {
        Some("turn") => {
            let msg = v
                .get("result")
                .and_then(|r| r.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            let mut s = state.lock().unwrap();
            if !msg.is_empty() {
                s.messages.push(ChatMsg { role: "jarvis".into(), text: msg });
            }
            s.thinking = false;
        }
        Some("history") => {
            if let Some(arr) = v.get("messages").and_then(|m| m.as_array()) {
                let mut s = state.lock().unwrap();
                s.messages = arr
                    .iter()
                    .filter_map(|m| {
                        let role = m.get("role")?.as_str()?.to_string();
                        let text = m.get("content")?.as_str()?.to_string();
                        (!text.is_empty()).then_some(ChatMsg { role, text })
                    })
                    .collect();
            }
        }
        _ => {} // ready / presence / tts : ignorés pour le chat texte
    }
    ctx.request_repaint();
}

/// Démarre le client chat (tâche tokio, reconnexion auto). Renvoie l'émetteur pour envoyer du texte.
pub fn spawn(
    rt: &tokio::runtime::Handle,
    gateway_url: String,
    state: Arc<Mutex<ChatState>>,
    ctx: egui::Context,
) -> tokio::sync::mpsc::UnboundedSender<String> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
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
                                Some(text) => {
                                    let frame = serde_json::json!({ "text": text }).to_string();
                                    if write.send(Message::Text(frame.into())).await.is_err() {
                                        break;
                                    }
                                }
                                None => return, // émetteur droppé → app fermée
                            },
                            inc = read.next() => match inc {
                                Some(Ok(Message::Text(t))) => handle_frame(&t, &state, &ctx),
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
