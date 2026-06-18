//! Persistance locale de l'uploader : dossiers de replays + ensemble des fichiers déjà uploadés.
//! (Le serveur/token NE sont PAS ici — fournis par l'app via aegis-core::Config ; rien de gravé.)
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// `%LOCALAPPDATA%/Aegis/uploader/` (ou équivalent). Crée le dossier au besoin.
pub fn data_dir() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("Aegis").join("uploader");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn uploaded_path() -> PathBuf {
    data_dir().join("uploaded.json")
}

/// Ensemble des noms de fichiers déjà uploadés (dédup local ; le serveur dédoublonne aussi).
pub fn load_uploaded() -> HashSet<String> {
    std::fs::read_to_string(uploaded_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_uploaded(uploaded: &HashSet<String>) {
    if let Ok(json) = serde_json::to_string(uploaded) {
        std::fs::write(uploaded_path(), json).ok();
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DirsFile {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    replay_dirs: Vec<String>,
}

fn dirs_path() -> PathBuf {
    data_dir().join("dirs.json")
}

/// Dossiers de replays configurés (persistés).
pub fn load_dirs() -> Vec<String> {
    std::fs::read_to_string(dirs_path())
        .ok()
        .and_then(|s| serde_json::from_str::<DirsFile>(&s).ok())
        .map(|d| d.replay_dirs)
        .unwrap_or_default()
}

pub fn save_dirs(dirs: &[String]) -> Result<(), String> {
    let f = DirsFile { replay_dirs: dirs.to_vec() };
    let json = serde_json::to_string_pretty(&f).map_err(|e| e.to_string())?;
    std::fs::write(dirs_path(), json).map_err(|e| e.to_string())
}
