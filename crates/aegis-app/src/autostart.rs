//! Démarrage automatique au login Windows via `HKCU\…\CurrentVersion\Run` (par utilisateur, sans
//! élévation). No-op sur les autres plateformes. La valeur pointe sur l'exe courant entre guillemets.
#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const VALUE_NAME: &str = "Aegis";

/// Le démarrage auto est-il activé (valeur présente dans la clé Run) ?
#[cfg(windows)]
pub fn is_enabled() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|k| k.get_value::<String, _>(VALUE_NAME))
        .is_ok()
}

/// Active/désactive le démarrage auto (écrit/supprime la valeur Run pointant sur l'exe courant).
#[cfg(windows)]
pub fn set(enabled: bool) -> std::io::Result<()> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY)?;
    if enabled {
        let exe = std::env::current_exe()?;
        key.set_value(VALUE_NAME, &format!("\"{}\"", exe.display()))
    } else {
        // Absence = déjà désactivé : on ignore l'erreur "introuvable".
        let _ = key.delete_value(VALUE_NAME);
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn is_enabled() -> bool {
    false
}

#[cfg(not(windows))]
pub fn set(_enabled: bool) -> std::io::Result<()> {
    Ok(())
}
