//! Audio natif pour la voix « hey Jarvis » : capture micro (cpal) → frames 16 kHz PCM16 streamées
//! à la gateway, et lecture du TTS (rodio). Le wakeword/VAD/STT/TTS sont côté Jarvis ; Aegis ne fait
//! que capter, ré-échantillonner vers 16 kHz mono, envoyer en continu, et jouer le wav retourné.
//!
//! NB : non testable hors-machine (pas de périphérique audio ici) → réglages device/rate à ajuster
//! sur la machine cible si besoin. Tolérant : toute erreur d'init coupe la voix sans crasher l'app.
use crate::chat::Outbound;
use base64::Engine;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const TARGET_RATE: f32 = 16_000.0;
const FRAME_SAMPLES: usize = 1280; // 80 ms @ 16 kHz (chunk attendu par openwakeword)

/// Ré-échantillonneur streaming device→16 kHz mono + accumulateur de frames PCM16 → base64.
struct Resampler {
    ratio: f32, // device_rate / 16000
    channels: usize,
    pos: f32,        // position fractionnaire dans le flux mono (état conservé entre callbacks)
    mono: Vec<f32>,  // mono device-rate en attente de ré-échantillonnage
    frame: Vec<i16>, // sortie 16 kHz en attente d'atteindre FRAME_SAMPLES
    tx: tokio::sync::mpsc::UnboundedSender<Outbound>,
}

impl Resampler {
    fn push_mono(&mut self, samples: &[f32]) {
        self.mono.extend_from_slice(samples);
        // Ré-échantillonnage linéaire jusqu'à épuisement (en gardant 1 échantillon de marge).
        while (self.pos as usize) + 1 < self.mono.len() {
            let i = self.pos as usize;
            let frac = self.pos - i as f32;
            let v = self.mono[i] * (1.0 - frac) + self.mono[i + 1] * frac;
            self.frame.push((v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
            self.pos += self.ratio;
            if self.frame.len() >= FRAME_SAMPLES {
                self.flush();
            }
        }
        // Conserve la queue non consommée + recale `pos`.
        let consumed = self.pos as usize;
        if consumed > 0 {
            self.mono.drain(0..consumed.min(self.mono.len()));
            self.pos -= consumed as f32;
        }
    }

    fn flush(&mut self) {
        let n = self.frame.len().min(FRAME_SAMPLES);
        let mut bytes = Vec::with_capacity(n * 2);
        for s in self.frame.drain(0..n) {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let _ = self.tx.send(Outbound::Wake(b64));
    }

    /// Données interleaved (f32) → mono (moyenne des canaux) → push.
    fn feed_f32(&mut self, data: &[f32]) {
        let ch = self.channels.max(1);
        let mono: Vec<f32> = data.chunks(ch).map(|c| c.iter().sum::<f32>() / ch as f32).collect();
        self.push_mono(&mono);
    }
    fn feed_i16(&mut self, data: &[i16]) {
        let ch = self.channels.max(1);
        let mono: Vec<f32> = data
            .chunks(ch)
            .map(|c| c.iter().map(|&s| s as f32 / i16::MAX as f32).sum::<f32>() / ch as f32)
            .collect();
        self.push_mono(&mono);
    }
}

/// Handle de capture ; le drop coupe la capture (le stream cpal est libéré dans son thread).
pub struct ListenHandle {
    stop: Arc<AtomicBool>,
}
impl Drop for ListenHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Démarre la capture micro en continu → frames 16 kHz vers `tx`. None si pas de micro/format géré.
pub fn start_listening(tx: tokio::sync::mpsc::UnboundedSender<Outbound>) -> Option<ListenHandle> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    std::thread::spawn(move || {
        let host = cpal::default_host();
        let Some(device) = host.default_input_device() else {
            eprintln!("voice: aucun micro par défaut");
            return;
        };
        let Ok(supported) = device.default_input_config() else {
            eprintln!("voice: pas de config d'entrée");
            return;
        };
        let rate = supported.sample_rate().0 as f32;
        let channels = supported.channels() as usize;
        let mut rs = Resampler {
            ratio: rate / TARGET_RATE,
            channels,
            pos: 0.0,
            mono: Vec::new(),
            frame: Vec::new(),
            tx,
        };
        let cfg = supported.config();
        let err = |e| eprintln!("voice: erreur stream: {e}");
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &cfg,
                move |data: &[f32], _: &_| rs.feed_f32(data),
                err,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &cfg,
                move |data: &[i16], _: &_| rs.feed_i16(data),
                err,
                None,
            ),
            other => {
                eprintln!("voice: format non géré ({other:?})");
                return;
            }
        };
        let Ok(stream) = stream else {
            eprintln!("voice: build_input_stream a échoué");
            return;
        };
        if stream.play().is_err() {
            eprintln!("voice: stream.play a échoué");
            return;
        }
        // Garde le stream vivant jusqu'au stop.
        while !stop_thread.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
    });
    Some(ListenHandle { stop })
}

/// Lecteur TTS : un thread possède le périphérique de sortie (rodio) et joue séquentiellement les
/// wav reçus. Clonable (partage l'émetteur).
#[derive(Clone)]
pub struct Player {
    tx: std::sync::mpsc::Sender<Vec<u8>>,
}
impl Player {
    pub fn new() -> Player {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let Ok((_stream, handle)) = rodio::OutputStream::try_default() else {
                eprintln!("voice: pas de sortie audio");
                return;
            };
            // _stream doit rester vivant tant que le thread tourne (sinon plus de son).
            while let Ok(wav) = rx.recv() {
                if let Ok(sink) = rodio::Sink::try_new(&handle) {
                    if let Ok(dec) = rodio::Decoder::new(std::io::Cursor::new(wav)) {
                        sink.append(dec);
                        sink.sleep_until_end();
                    }
                }
            }
        });
        Player { tx }
    }
    pub fn play(&self, wav: Vec<u8>) {
        let _ = self.tx.send(wav);
    }
}
