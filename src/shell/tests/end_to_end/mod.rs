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
mod blocks;
mod outline;
mod boards;
mod cursors_comments;
mod dates_lists;
mod table_handles;
mod table_styles;
mod table_formulas;
mod embeds;
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
        self.shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string())
    }

    /// Loads `text` in the active editor, the cursor at byte `cursor`.
    pub fn load(&self, cx: &mut gpui::VisualTestContext, text: &str, cursor: usize) {
        self.shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.load(text.to_string(), cursor, cx)));
        cx.run_until_parked();
    }

    /// Kind of the line at byte `at` in the active editor.
    pub fn kind(&self, cx: &mut gpui::VisualTestContext, at: usize) -> Option<markdown::Kind> {
        self.shell.read_with(cx, |s, cx| s.pane().editor.read(cx).kind_at(at))
    }

    /// Lets the vault watcher see what changed on disk.
    pub fn settle(&self, cx: &mut gpui::VisualTestContext) {
        for _ in 0..2 {
            cx.executor().advance_clock(Duration::from_secs(3));
            cx.run_until_parked();
        }
    }
}

/// `secondary` = Cmd on macOS, Ctrl elsewhere, as in `bind_keys`.
/// Full keyboard run: typing, lists, saving, palette, wikilink.
#[gpui::test]
fn end_to_end(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-e2e-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("Courses.md"), "# Courses\n\n- lait #maison\n").unwrap();
    fs::create_dir(root.join("Projets")).unwrap();
    fs::write(root.join("Projets/Plan.md"), "# Plan\n\n[[Courses]]\n").unwrap();
        // The test config must not touch the user's.
    let _config = isolated_config(&root);
        // What a version named "encre" left is read again as long as nothing exists under the new name.
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
    blocks::headings(&j, cx);
    blocks::run(&j, cx);
    outline::run(&j, cx);
    boards::run(&j, cx, &here);
    cursors_comments::run(&j, cx);
    dates_lists::run(&j, cx);
    table_handles::run(&j, cx);
    table_styles::run(&j, cx);
    table_formulas::run(&j, cx);
    embeds::run(&j, cx);
    find_mouse::run(&j, cx);
    csv::run(&j, cx);
    history_panes::run(&j, cx);
    git_sync::run(&j, cx);
    fs::remove_dir_all(&j.root).unwrap();
}
