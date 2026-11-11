//! Tests that drive the whole app (GPUI's test platform, no screen): keystrokes go in, the
//! shell, the views and the vault on disk are checked. Pure functions are tested in their own
//! modules instead.

mod benches;
mod board_header;
mod calendar_head;
mod canvas_nesting;
mod sheet_header;
mod sheet_widths;
mod table_open;
mod docks;
mod drops;
mod editing;
mod export_pdf;
mod enlarge;
mod end_to_end;
mod feedback;
mod floating;
mod find_bar;
mod vault_find;
mod vault_switch;
mod keyboard;
mod keycaps;
mod layers;
mod layout;
mod motion;
mod settings_page;
mod shortcuts_page;
mod palette_commands;
mod panel_rows;
mod canvas_text;
mod current_file;
mod panes;
mod render_pure;
mod paste_failure;
mod tabs;
mod save_close;
mod sharing;
mod window;

use super::*;
use crate::{logic::block, ui::theme::contrast, views::editor::grid};
use gpui::{Hsla, TestAppContext};

/// The vault watcher looks at the disk every 2 s (`shell/library.rs`): a little more than one round.
// ponytail: copied from library.rs, whose timers are not exported; export them there and use them here.
pub(super) const WATCH: Duration = Duration::from_secs(3);
/// A note's autosave (`SAVE_DELAY`) with some margin; the table's, 400 ms too, fires within it.
pub(super) const SAVED: Duration = notes::SAVE_DELAY.saturating_add(Duration::from_millis(100));

/// Lets the vault watcher see what changed on disk: two of its rounds, the window drawn after each.
pub(super) fn watch(cx: &mut gpui::VisualTestContext) {
    for _ in 0..2 {
        cx.executor().advance_clock(WATCH);
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

/// Frames until the layout settles, without the clock moving (toasts and fades keep their time).
pub(super) fn settle(cx: &mut gpui::VisualTestContext) {
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

/// What tests ask of the panel and what they set up in it, so that they do not reach into `Nav`.
impl Shell {
    /// The panel's tool and its size: (tool, panel mode).
    pub(super) fn panel_view(&self) -> (crate::views::nav::Mode, crate::views::nav::Panel) {
        (self.nav.mode(), self.nav.panel())
    }

    /// Shows `mode` alone in the panel, in `panel` mode.
    pub(super) fn show_panel(&mut self, mode: crate::views::nav::Mode, panel: crate::views::nav::Panel, cx: &mut Context<Self>) {
        self.nav.show_alone(mode);
        self.nav.set_panel(panel);
        cx.notify();
    }
}

/// Every file under `dir`, folders walked.
fn files(dir: &Path) -> Vec<PathBuf> {
    let entries = fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().path());
    entries.flat_map(|path| if path.is_dir() { files(&path) } else { vec![path] }).collect()
}

/// The shapes come from the theme (`t.small()`, `t.large()`, `t.lifted()`, `t.floating()`, `t.hover`): a view that
/// writes a raw radius, a raw shadow or a hover in the colour of the lines brings back the drift of seven radii.
#[test]
fn shapes_come_from_the_theme() {
    let banned = [".rounded(px(", ".rounded(t.px(", "BoxShadow", ".shadow_sm", ".shadow_md", ".shadow_lg", ".shadow_xs", ".shadow_2xs", ".shadow_none", "s.bg(t.border)"];
    for path in files(Path::new("src/views")).into_iter().chain(files(Path::new("src/shell"))).chain(files(Path::new("src/ui"))) {
        if path.starts_with("src/shell/tests") || path.ends_with("theme.rs") || path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let code = fs::read_to_string(&path).unwrap();
        let code = code.split("#[cfg(test)]\nmod tests").next().unwrap();
        for (n, line) in code.lines().map(|l| l.split("//").next().unwrap()).enumerate() {
            for word in banned {
                assert!(!line.contains(word), "{}:{}: `{word}`, use the theme's shapes", path.display(), n + 1);
            }
        }
    }
}

/// The French interface speaks to the reader as « vous » (request of 2026-10-23): no « tu » form left in
/// a text of the app. Action labels stay in the infinitive.
#[test]
fn french_says_vous() {
    let tu = [" tes ", " ton ", " ta ", "Tes ", "Ton ", " tu ", "Tu ", "-toi", " t'", "réessaie", "vérifie ", "installe-le", "Tape ", "Écris ", " écris ", "Ouvre ", "Choisis ", " fais ", "Rouvre-", "commence-"];
    for path in files(Path::new("src")).into_iter().filter(|p| !p.starts_with("src/shell/tests") && p.extension().is_some_and(|e| e == "rs")) {
        let code = fs::read_to_string(&path).unwrap();
        // The texts only: what is between quotes, outside comments and tests.
        let code = code.split("#[cfg(test)]\nmod tests").next().unwrap();
        for line in code.lines().map(|l| l.split("//").next().unwrap()) {
            for text in line.split('"').skip(1).step_by(2) {
                if let Some(word) = tu.iter().find(|w| text.contains(*w)) {
                    panic!("{}: « {text} » says « {} »", path.display(), word.trim());
                }
            }
        }
    }
}

/// The config folder is global to the process: a test that moves it under `root` holds the guard
/// until it ends, so that another does not move it while it uses it.
fn isolated_config(root: &Path) -> crate::platform::config::Isolated {
    crate::platform::config::isolate(root)
}

/// A temporary folder named after the test, the process and a counter, empty at the start and removed when it goes out of
/// scope, so a failing test leaves nothing behind either. It reads as a `PathBuf`.
pub(super) struct TempDir(PathBuf);

impl TempDir {
    pub(super) fn new(name: &str) -> Self {
        // A counter besides the name: two tests may pick the same name and run at the same time.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("bref-{name}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl std::ops::Deref for TempDir {
    type Target = PathBuf;
    fn deref(&self) -> &PathBuf {
        &self.0
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A shell on a fresh vault in a temporary folder.
pub(super) fn start<'a>(cx: &'a mut TestAppContext, name: &str) -> (TempDir, Entity<Shell>, &'a mut gpui::VisualTestContext, crate::platform::config::Isolated) {
    let root = TempDir::new(name);
    let config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx.run_until_parked();
    (root, shell, cx, config)
}

/// Selectors are `&'static str`: the few built by the tests are leaked.
pub(super) fn leak(name: String) -> &'static str {
    Box::leak(name.into_boxed_str())
}
