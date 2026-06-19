//! Config : endpoints + tokens + périphériques audio + phrases voix, persistés en local. Couche
//! env > settings.json > défaut (même philosophie que l'uploader). Rien n'est gravé dans le binaire.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jarvis_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jarvis_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storm_codex_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orpheus_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hots_patch_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_token: Option<String>,
    // Voix : périphériques choisis (par nom cpal ; None = défaut système) + phrases d'accusé.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mic_device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wake_reply: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_reply: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub jarvis_url: String,
    pub jarvis_token: Option<String>,
    pub storm_codex_url: String,
    pub orpheus_url: String,
    /// URL du service HotsPatchNotes (optionnel) — sondé sur le Dashboard si défini.
    pub hots_patch_url: Option<String>,
    pub upload_token: Option<String>,
    pub mic_device: Option<String>,
    pub output_device: Option<String>,
    pub wake_reply: String,
    pub thinking_reply: String,
}

impl Config {
    pub const DEFAULT_JARVIS: &'static str = "http://localhost:8080";
    pub const DEFAULT_STORM: &'static str = "http://localhost:5102";
    pub const DEFAULT_ORPHEUS: &'static str = "http://localhost:3010";
    pub const DEFAULT_WAKE_REPLY: &'static str = "Oui ?";
    pub const DEFAULT_THINKING_REPLY: &'static str = "Une seconde…";

    /// Résolution env > fichier > défaut. `env` est injecté pour la testabilité. Chaînes vides = non défini.
    pub fn resolve(file: ConfigFile, env: impl Fn(&str) -> Option<String>) -> Config {
        let pick = |envk: &str, f: Option<String>, default: &str| {
            env(envk).or(f).filter(|s| !s.trim().is_empty()).unwrap_or_else(|| default.to_string())
        };
        let opt = |envk: &str, f: Option<String>| env(envk).or(f).filter(|s| !s.trim().is_empty());
        Config {
            jarvis_url: pick("AEGIS_JARVIS_URL", file.jarvis_url, Self::DEFAULT_JARVIS),
            jarvis_token: opt("AEGIS_JARVIS_TOKEN", file.jarvis_token),
            storm_codex_url: pick("AEGIS_STORM_URL", file.storm_codex_url, Self::DEFAULT_STORM),
            orpheus_url: pick("AEGIS_ORPHEUS_URL", file.orpheus_url, Self::DEFAULT_ORPHEUS),
            hots_patch_url: opt("AEGIS_HOTS_URL", file.hots_patch_url),
            upload_token: opt("AEGIS_UPLOAD_TOKEN", file.upload_token),
            mic_device: opt("AEGIS_MIC", file.mic_device),
            output_device: opt("AEGIS_OUTPUT", file.output_device),
            wake_reply: pick("AEGIS_WAKE_REPLY", file.wake_reply, Self::DEFAULT_WAKE_REPLY),
            thinking_reply: pick("AEGIS_THINKING_REPLY", file.thinking_reply, Self::DEFAULT_THINKING_REPLY),
        }
    }

    pub fn config_path() -> PathBuf {
        let dir = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("Aegis");
        std::fs::create_dir_all(&dir).ok();
        dir.join("config.json")
    }

    pub fn load() -> Config {
        let file = std::fs::read_to_string(Self::config_path())
            .ok()
            .and_then(|s| serde_json::from_str::<ConfigFile>(&s).ok())
            .unwrap_or_default();
        Self::resolve(file, |k| std::env::var(k).ok())
    }

    pub fn save(&self) -> Result<(), String> {
        let trim = |s: &str| {
            let t = s.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        let f = ConfigFile {
            jarvis_url: trim(&self.jarvis_url),
            jarvis_token: self.jarvis_token.clone().filter(|s| !s.trim().is_empty()),
            storm_codex_url: trim(&self.storm_codex_url),
            orpheus_url: trim(&self.orpheus_url),
            hots_patch_url: self.hots_patch_url.clone().filter(|s| !s.trim().is_empty()),
            upload_token: self.upload_token.clone().filter(|s| !s.trim().is_empty()),
            mic_device: self.mic_device.clone().filter(|s| !s.trim().is_empty()),
            output_device: self.output_device.clone().filter(|s| !s.trim().is_empty()),
            wake_reply: trim(&self.wake_reply),
            thinking_reply: trim(&self.thinking_reply),
        };
        let json = serde_json::to_string_pretty(&f).map_err(|e| e.to_string())?;
        std::fs::write(Self::config_path(), json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layering_prefers_env_then_file_then_default() {
        let file = ConfigFile {
            storm_codex_url: Some("http://file:5102".into()),
            upload_token: Some("tok".into()),
            ..Default::default()
        };
        let c = Config::resolve(file, |k| match k {
            "AEGIS_JARVIS_URL" => Some("http://env:8080".into()),
            _ => None,
        });
        assert_eq!(c.jarvis_url, "http://env:8080");
        assert_eq!(c.storm_codex_url, "http://file:5102");
        assert_eq!(c.orpheus_url, Config::DEFAULT_ORPHEUS);
        assert_eq!(c.upload_token.as_deref(), Some("tok"));
        assert_eq!(c.wake_reply, Config::DEFAULT_WAKE_REPLY);
    }

    #[test]
    fn empty_strings_are_unset_and_replies_default() {
        let file = ConfigFile {
            jarvis_url: Some("".into()),
            jarvis_token: Some("".into()),
            wake_reply: Some("".into()),
            ..Default::default()
        };
        let c = Config::resolve(file, |_| None);
        assert_eq!(c.jarvis_url, Config::DEFAULT_JARVIS);
        assert_eq!(c.jarvis_token, None);
        assert_eq!(c.mic_device, None);
        assert_eq!(c.wake_reply, Config::DEFAULT_WAKE_REPLY);
        assert_eq!(c.thinking_reply, Config::DEFAULT_THINKING_REPLY);
    }
}
