//! Tests that drive the whole app (GPUI's test platform, no screen): keystrokes go in, the
//! shell, the views and the vault on disk are checked. Pure functions are tested in their own
//! modules instead.

mod benches;
mod board_header;
mod calendar_head;
mod sheet_header;
mod table_open;
mod docks;
mod drops;
mod editing;
mod enlarge;
mod end_to_end;
mod feedback;
mod floating;
mod find_bar;
mod vault_find;
mod keyboard;
mod keycaps;
mod layout;
mod motion;
mod panel_rows;
mod panes;
mod tabs;
mod window;

use super::*;
use crate::{logic::block, ui::theme::contrast, views::editor::grid};
use gpui::{Hsla, TestAppContext};

static CONFIG: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The layers only depend downwards: `logic` and `platform` never see gpui nor the layers
/// above them, `ui` never sees the views, and the views never see the shell, whose code stays
/// in `src/shell`. See ARCHITECTURE.md.
#[test]
fn layers_only_depend_downwards() {
    let rules: &[(&str, &[&str])] = &[
        ("src/logic", &["gpui", "crate::platform", "crate::ui", "crate::views", "crate::shell"]),
        ("src/platform", &["gpui", "crate::ui", "crate::views", "crate::shell"]),
        ("src/ui", &["crate::views", "crate::shell", "impl Shell"]),
        ("src/views", &["crate::shell", "impl Shell"]),
    ];
    fn files(dir: &Path) -> Vec<PathBuf> {
        let entries = fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().path());
        entries.flat_map(|path| if path.is_dir() { files(&path) } else { vec![path] }).collect()
    }
    for (dir, banned) in rules {
        for path in files(Path::new(dir)) {
            let code = fs::read_to_string(&path).unwrap();
            // Comments may name anything; only code counts.
            let code: String = code.lines().map(|l| l.split("//").next().unwrap()).collect::<Vec<_>>().join("\n");
            for word in *banned {
                assert!(!code.contains(word), "{} uses `{word}`", path.display());
            }
        }
    }
}

/// The shapes come from the theme (`t.small()`, `t.large()`, `t.lifted()`, `t.floating()`, `t.hover`): a view that
/// writes a raw radius, a raw shadow or a hover in the colour of the lines brings back the drift of seven radii.
#[test]
fn shapes_come_from_the_theme() {
    fn files(dir: &Path) -> Vec<PathBuf> {
        let entries = fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().path());
        entries.flat_map(|path| if path.is_dir() { files(&path) } else { vec![path] }).collect()
    }
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
    fn files(dir: &Path) -> Vec<PathBuf> {
        let entries = fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().path());
        entries.flat_map(|path| if path.is_dir() { files(&path) } else { vec![path] }).collect()
    }
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

/// The config folder is global to the process (environment variables): a test that moves it under
/// `root` holds the lock until it ends, so that another does not move it while it uses it.
fn isolated_config(root: &Path) -> std::sync::MutexGuard<'static, ()> {
    let guard = CONFIG.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    for key in ["XDG_CONFIG_HOME", "HOME", "APPDATA", "USERPROFILE"] {
        unsafe { std::env::set_var(key, root.join(".config")) };
    }
    guard
}

/// A shell on a fresh vault in a temporary folder.
pub(super) fn start<'a>(cx: &'a mut TestAppContext, name: &str) -> (PathBuf, Entity<Shell>, &'a mut gpui::VisualTestContext, std::sync::MutexGuard<'static, ()>) {
    let root = std::env::temp_dir().join(format!("bref-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
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
