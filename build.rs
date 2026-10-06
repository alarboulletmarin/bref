// Icône de l'exécutable Windows : GPUI charge la ressource n° 1 pour la fenêtre et la barre des tâches.
fn main() {
    #[cfg(windows)]
    embed_resource::compile("packaging/windows/bref.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
