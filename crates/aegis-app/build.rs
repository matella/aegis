//! Embarque l'icône Aegis comme ressource Windows dans l'exe (barre des tâches, Explorateur,
//! raccourcis). Sans effet sur les autres plateformes. Échec silencieux si rc.exe est absent →
//! le build n'est jamais bloqué (l'icône runtime egui reste en repli).
fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=../../assets/aegis.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/aegis.ico");
        let _ = res.compile();
    }
}
