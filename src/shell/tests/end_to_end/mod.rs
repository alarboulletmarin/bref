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

use crate::platform::config;
use super::*;

/// What every chapter works on: the window's shell and the vault on disk.
pub(super) struct Journey {
    pub shell: Entity<Shell>,
    pub root: TempDir,
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
        super::watch(cx);
    }
}

/// Runs one chapter; a panic (a failed assertion) is kept with the chapter's name instead of ending the test.
fn chapter(failed: &mut Vec<(&'static str, String)>, name: &'static str, run: impl FnOnce()) {
    if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)) {
        let why = panic.downcast_ref::<String>().cloned().or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()));
        failed.push((name, why.unwrap_or_else(|| "panicked".into())));
    }
}

/// `secondary` = Cmd on macOS, Ctrl elsewhere, as in `bind_keys`.
/// Full keyboard run: typing, lists, saving, palette, wikilink.
#[gpui::test]
fn end_to_end(cx: &mut TestAppContext) {
    let root = TempDir::new("e2e");
    fs::write(root.join("Courses.md"), "# Courses\n\n- lait #maison\n").unwrap();
    fs::create_dir(root.join("Projets")).unwrap();
    fs::write(root.join("Projets/Plan.md"), "# Plan\n\n[[Courses]]\n").unwrap();
    // The test config must not touch the user's.
    let _config = isolated_config(&root);
    // What a version named "encre" left is read again as long as nothing exists under the new name.
    let old = config::legacy_path(config::File::Config).parent().unwrap().to_path_buf();
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("config"), format!("{}\n", root.display())).unwrap();
    fs::write(old.join("layout"), "graph split 260 0").unwrap();
    assert_eq!(config::load_config().0, Some(root.clone()));
    assert_eq!(config::load(config::File::Layout), "graph split 260 0");
    fs::remove_dir_all(&old).unwrap();

    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx.run_until_parked();
    let j = Journey { shell, root };
    // One session, for speed: a chapter that fails does not stop the next ones, and every failure is
    // reported at the end, by chapter. A chapter that fails may leave the session in a state that fails
    // the following ones: read the list from the top.
    let mut failed = Vec::new();
    chapter(&mut failed, "writing", || writing::run(&j, cx));
    chapter(&mut failed, "settings", || settings::run(&j, cx));
    chapter(&mut failed, "panel", || panel::run(&j, cx));
    chapter(&mut failed, "files", || files::run(&j, cx));
    chapter(&mut failed, "media", || media::run(&j, cx));
    let mut here = None;
    chapter(&mut failed, "vault_views", || here = Some(vault_views::run(&j, cx)));
    chapter(&mut failed, "diagrams", || diagrams::run(&j, cx));
    chapter(&mut failed, "md_tables", || md_tables::run(&j, cx));
    chapter(&mut failed, "blocks (headings)", || blocks::headings(&j, cx));
    chapter(&mut failed, "blocks (tags)", || blocks::tags(&j, cx));
    chapter(&mut failed, "blocks", || blocks::run(&j, cx));
    chapter(&mut failed, "outline", || outline::run(&j, cx));
    match &here {
        Some(here) => chapter(&mut failed, "boards", || boards::run(&j, cx, here)),
        None => failed.push(("boards", "skipped: it needs what vault_views returns".into())),
    }
    chapter(&mut failed, "cursors_comments", || cursors_comments::run(&j, cx));
    chapter(&mut failed, "dates_lists", || dates_lists::run(&j, cx));
    chapter(&mut failed, "table_handles", || table_handles::run(&j, cx));
    chapter(&mut failed, "table_styles", || table_styles::run(&j, cx));
    chapter(&mut failed, "table_formulas", || table_formulas::run(&j, cx));
    chapter(&mut failed, "embeds", || embeds::run(&j, cx));
    chapter(&mut failed, "find_mouse", || find_mouse::run(&j, cx));
    chapter(&mut failed, "csv", || csv::run(&j, cx));
    chapter(&mut failed, "history_panes", || history_panes::run(&j, cx));
    chapter(&mut failed, "git_sync", || git_sync::run(&j, cx));
    let report: Vec<String> = failed.iter().map(|(name, why)| format!("- {name}: {why}")).collect();
    assert!(failed.is_empty(), "{} chapter(s) failed:\n{}", failed.len(), report.join("\n"));
}
