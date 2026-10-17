//! The whole app driven by simulated keystrokes, chapter by chapter. The chapters run in order,
//! as one session: each starts from the vault, panel and settings the previous one left.

mod writing;
mod settings;
mod panel;
mod files;
mod media;
mod vault_views;
mod diagrams;
mod md_tables;
mod outline;
mod boards;
mod cursors_comments;
mod dates_lists;
mod table_handles;
mod find_mouse;
mod csv;
mod history_panes;
mod git_sync;

use super::*;

/// What every chapter works on: the window's shell and the vault on disk.
pub(super) struct Journey {
    pub shell: Entity<Shell>,
    pub root: PathBuf,
}

impl Journey {
    /// Text of the note in the active pane.
    pub fn text(&self, cx: &mut gpui::VisualTestContext) -> String {
        self.shell.read_with(cx, |s, cx| s.editor.read(cx).text().to_string())
    }

    /// Loads `text` in the active editor, the cursor at byte `cursor`.
    pub fn load(&self, cx: &mut gpui::VisualTestContext, text: &str, cursor: usize) {
        self.shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| e.load(text.to_string(), cursor, cx)));
        cx.run_until_parked();
    }

    /// Kind of the line at byte `at` in the active editor.
    pub fn kind(&self, cx: &mut gpui::VisualTestContext, at: usize) -> Option<markdown::Kind> {
        self.shell.read_with(cx, |s, cx| s.editor.read(cx).kind_at(at))
    }

    /// Lets the vault watcher see what changed on disk.
    pub fn settle(&self, cx: &mut gpui::VisualTestContext) {
        for _ in 0..2 {
            cx.executor().advance_clock(Duration::from_secs(3));
            cx.run_until_parked();
        }
    }
}

/// `secondary` = Cmd sur macOS, Ctrl ailleurs, comme dans `bind_keys`.
/// Parcours complet au clavier : saisie, listes, enregistrement, palette, wikilien.
#[gpui::test]
fn end_to_end(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-e2e-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("Courses.md"), "# Courses\n\n- lait #maison\n").unwrap();
    fs::create_dir(root.join("Projets")).unwrap();
    fs::write(root.join("Projets/Plan.md"), "# Plan\n\n[[Courses]]\n").unwrap();
    // La config de test ne doit pas toucher celle de l'utilisateur.
    let _config = isolated_config(&root);
    // Ce qu'une version nommée « encre » a laissé est relu tant que rien n'existe sous le nouveau nom.
    let old = vault::config_dir().join("encre");
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("config"), format!("{}\n", root.display())).unwrap();
    fs::write(old.join("layout"), "graph split 260 0").unwrap();
    assert_eq!(vault::load_config().0, Some(root.clone()));
    assert_eq!(vault::load_layout(), "graph split 260 0");
    fs::remove_dir_all(&old).unwrap();

    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx.run_until_parked();
    let j = Journey { shell, root };
    writing::run(&j, cx);
    settings::run(&j, cx);
    panel::run(&j, cx);
    files::run(&j, cx);
    media::run(&j, cx);
    let here = vault_views::run(&j, cx);
    diagrams::run(&j, cx);
    md_tables::run(&j, cx);
    outline::run(&j, cx);
    boards::run(&j, cx, &here);
    cursors_comments::run(&j, cx);
    dates_lists::run(&j, cx);
    table_handles::run(&j, cx);
    find_mouse::run(&j, cx);
    csv::run(&j, cx);
    history_panes::run(&j, cx);
    git_sync::run(&j, cx);
    fs::remove_dir_all(&j.root).unwrap();
}
