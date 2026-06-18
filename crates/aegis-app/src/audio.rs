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

/// Liste les micros disponibles (noms cpal). Vide si l'hôte ne répond pas.
pub fn input_devices() -> Vec<String> {
    cpal::default_host()
        .input_devices()
        .map(|it| it.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

/// Liste les sorties audio disponibles (noms cpal).
pub fn output_devices() -> Vec<String> {
    cpal::default_host()
        .output_devices()
        .map(|it| it.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

fn pick_input(host: &cpal::Host, name: &Option<String>) -> Option<cpal::Device> {
    if let Some(n) = name.as_deref().filter(|n| !n.is_empty()) {
        if let Ok(devs) = host.input_devices() {
            for d in devs {
                if d.name().ok().as_deref() == Some(n) {
                    return Some(d);
                }
            }
        }
    }
    host.default_input_device()
}

fn pick_output(host: &cpal::Host, name: &Option<String>) -> Option<cpal::Device> {
    if let Some(n) = name.as_deref().filter(|n| !n.is_empty()) {
        if let Ok(devs) = host.output_devices() {
            for d in devs {
                if d.name().ok().as_deref() == Some(n) {
                    return Some(d);
                }
            }
        }
    }
    host.default_output_device()
}

/// Parle une phrase courte via la synthèse vocale de l'OS (accusés voix). Best-effort, non bloquant.
pub fn speak(phrase: &str) {
    let phrase = phrase.trim();
    if phrase.is_empty() {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("say").arg(phrase).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let safe = phrase.replace('\'', " ");
        let script = format!(
            "Add-Type -AssemblyName System.Speech; (New-Object System.Speech.Synthesis.SpeechSynthesizer).Speak('{safe}')"
        );
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .spawn();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = std::process::Command::new("spd-say").arg(phrase).spawn();
    }
}

/// Démarre la capture micro en continu → frames 16 kHz vers `tx`. `mic` = nom du périphérique (None
/// = défaut). None si pas de micro/format géré.
pub fn start_listening(
    tx: tokio::sync::mpsc::UnboundedSender<Outbound>,
    mic: Option<String>,
) -> Option<ListenHandle> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    std::thread::spawn(move || {
        let host = cpal::default_host();
        let Some(device) = pick_input(&host, &mic) else {
            eprintln!("voice: aucun micro");
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

enum Cmd {
    Wav(Vec<u8>),
    Chime(f32), // bip court (accusé wake/done) à la fréquence donnée
}

/// Lecteur audio : un thread possède le périphérique de sortie (rodio) et joue séquentiellement les
/// wav (TTS) et chimes reçus. `output` = nom du périphérique (None = défaut). Clonable.
#[derive(Clone)]
pub struct Player {
    tx: std::sync::mpsc::Sender<Cmd>,
}
impl Player {
    pub fn new(output: Option<String>) -> Player {
        let (tx, rx) = std::sync::mpsc::channel::<Cmd>();
        std::thread::spawn(move || {
            use rodio::Source;
            let host = cpal::default_host();
            let stream = pick_output(&host, &output)
                .and_then(|d| rodio::OutputStream::try_from_device(&d).ok())
                .or_else(|| rodio::OutputStream::try_default().ok());
            let Some((_stream, handle)) = stream else {
                eprintln!("voice: pas de sortie audio");
                return;
            };
            // _stream doit rester vivant tant que le thread tourne (sinon plus de son).
            while let Ok(cmd) = rx.recv() {
                let Ok(sink) = rodio::Sink::try_new(&handle) else { continue };
                match cmd {
                    Cmd::Wav(wav) => {
                        if let Ok(dec) = rodio::Decoder::new(std::io::Cursor::new(wav)) {
                            sink.append(dec);
                            sink.sleep_until_end();
                        }
                    }
                    Cmd::Chime(freq) => {
                        let s = rodio::source::SineWave::new(freq)
                            .take_duration(std::time::Duration::from_millis(140))
                            .amplify(0.18);
                        sink.append(s);
                        sink.sleep_until_end();
                    }
                }
            }
        });
        Player { tx }
    }
    pub fn play(&self, wav: Vec<u8>) {
        let _ = self.tx.send(Cmd::Wav(wav));
    }
    pub fn chime(&self, freq: f32) {
        let _ = self.tx.send(Cmd::Chime(freq));
    }
}
