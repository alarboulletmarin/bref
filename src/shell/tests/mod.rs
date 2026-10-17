//! Tests that drive the whole app (GPUI's test platform, no screen): keystrokes go in, the
//! shell, the views and the vault on disk are checked. Pure functions are tested in their own
//! modules instead.

mod benches;
mod drops;
mod editing;
mod end_to_end;
mod layout;
mod window;

use super::*;
use crate::{logic::block, ui::theme::contrast, views::grid};
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
    for (dir, banned) in rules {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let code = fs::read_to_string(&path).unwrap();
            // Comments may name anything; only code counts.
            let code: String = code.lines().map(|l| l.split("//").next().unwrap()).collect::<Vec<_>>().join("\n");
            for word in *banned {
                assert!(!code.contains(word), "{} uses `{word}`", path.display());
            }
        }
    }
}

/// Le dossier de config est global au processus (variables d'environnement) : un test qui le déplace sous
/// `root` garde le verrou jusqu'à sa fin, pour qu'un autre ne le déplace pas pendant qu'il s'en sert.
fn isolated_config(root: &Path) -> std::sync::MutexGuard<'static, ()> {
    let guard = CONFIG.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    for key in ["XDG_CONFIG_HOME", "HOME", "APPDATA", "USERPROFILE"] {
        unsafe { std::env::set_var(key, root.join(".config")) };
    }
    guard
}
