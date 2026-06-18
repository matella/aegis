//! Config : endpoints + token, persistés en local. Couche env > settings.json > défaut (même
//! philosophie que l'uploader client-rs). Rien n'est gravé dans le binaire.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jarvis_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storm_codex_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orpheus_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_token: Option<String>,
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

    /// Résolution env > fichier > défaut. `env` est injecté pour la testabilité. Les chaînes vides
    /// sont traitées comme « non défini ».
    pub fn resolve(file: ConfigFile, env: impl Fn(&str) -> Option<String>) -> Config {
        let pick = |envk: &str, f: Option<String>, default: &str| {
            env(envk)
                .or(f)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| default.to_string())
        };
        let upload_token = env("AEGIS_UPLOAD_TOKEN")
            .or(file.upload_token)
            .filter(|s| !s.trim().is_empty());
        Config {
            jarvis_url: pick("AEGIS_JARVIS_URL", file.jarvis_url, Self::DEFAULT_JARVIS),
            storm_codex_url: pick("AEGIS_STORM_URL", file.storm_codex_url, Self::DEFAULT_STORM),
            orpheus_url: pick("AEGIS_ORPHEUS_URL", file.orpheus_url, Self::DEFAULT_ORPHEUS),
            upload_token,
        }
    }

    pub fn config_path() -> PathBuf {
        let dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Aegis");
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
            storm_codex_url: trim(&self.storm_codex_url),
            orpheus_url: trim(&self.orpheus_url),
            upload_token: self.upload_token.clone().filter(|s| !s.trim().is_empty()),
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
            jarvis_url: None,
            storm_codex_url: Some("http://file:5102".into()),
            orpheus_url: None,
            upload_token: Some("tok".into()),
        };
        let c = Config::resolve(file, |k| match k {
            "AEGIS_JARVIS_URL" => Some("http://env:8080".into()),
            _ => None,
        });
        assert_eq!(c.jarvis_url, "http://env:8080");
        assert_eq!(c.storm_codex_url, "http://file:5102");
        assert_eq!(c.orpheus_url, Config::DEFAULT_ORPHEUS);
        assert_eq!(c.upload_token.as_deref(), Some("tok"));
    }

    #[test]
    fn empty_strings_are_unset() {
        let file = ConfigFile {
            jarvis_url: Some("".into()),
            storm_codex_url: None,
            orpheus_url: None,
            upload_token: Some("".into()),
        };
        let c = Config::resolve(file, |_| None);
        assert_eq!(c.jarvis_url, Config::DEFAULT_JARVIS);
        assert_eq!(c.upload_token, None);
    }
}
