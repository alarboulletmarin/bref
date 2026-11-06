//! The app's own files, in the user's configuration folder (one folder, `bref`).

use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use super::vault;
use crate::lang::tr;

/// Name of the app's folder under the configuration folder, and the one of its old name.
const FOLDER: &str = "bref";
const OLD_FOLDER: &str = "encre";

/// The files kept in the configuration folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum File {
    /// The vault (first line) and the recent notes: one path per line.
    Config,
    /// Layout of the navigation panel, on one line.
    Layout,
    /// Chosen appearance (theme, fonts, size): one `key=value` line per setting.
    Settings,
    /// Result of the last new-version check.
    Update,
    /// What is kept for each table (format, column widths).
    Tables,
    /// Changed shortcuts.
    Keys,
    /// Panes of the window.
    Panes,
    /// The last panic.
    Crash,
    /// Door of the agent (port and secret).
    Agent,
}

impl File {
    fn name(self) -> &'static str {
        match self {
            File::Config => "config",
            File::Layout => "layout",
            File::Settings => "settings",
            File::Update => "update",
            File::Tables => "tables",
            File::Keys => "keys",
            File::Panes => "panes",
            File::Crash => "crash",
            File::Agent => "agent",
        }
    }
}

fn from_environment() -> PathBuf {
    let var = |key: &str| env::var_os(key).map(PathBuf::from);
    let dir = if cfg!(windows) {
        var("APPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|home| home.join("Library/Application Support"))
    } else {
        var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|home| home.join(".config")))
    };
    dir.unwrap_or_default()
}

/// The user's configuration folder, per platform. Read from the environment once; tests
/// move it under a temporary folder (`isolate`).
pub fn config_dir() -> PathBuf {
    #[cfg(test)]
    return match isolated_root() {
        Some(root) => root.join(".config"),
        None => from_environment(),
    };
    #[cfg(not(test))]
    {
        static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        DIR.get_or_init(from_environment).clone()
    }
}

/// The user's home folder (`USERPROFILE` on Windows, `HOME` elsewhere).
pub fn home() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(root) = isolated_root() {
        return Some(root.join(".config"));
    }
    env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// Where tests put the home and config folders (both `<root>/.config`, as the environment variables
/// they replaced did), if one does.
#[cfg(test)]
static ISOLATED: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);
/// One test at a time moves the folders: the others wait for the guard to drop.
#[cfg(test)]
static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Tests never touch the user's real folders: the last isolated root stays in force after its guard
/// drops (a window torn down later still writes its files there), and before any test isolated one,
/// a throwaway folder stands in.
#[cfg(test)]
fn isolated_root() -> Option<PathBuf> {
    let set = ISOLATED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    set.or_else(|| Some(env::temp_dir().join(format!("bref-test-home-{}", std::process::id()))))
}

/// Test: the config and home folders live under `root`, and no other test moves them until the guard
/// drops. No environment variable is touched (changing it while other threads read it is unsound);
/// the override is process-wide, not per thread, so that threads a test starts see it too.
#[cfg(test)]
pub fn isolate(root: &Path) -> Isolated {
    let turn = TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *ISOLATED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(root.to_path_buf());
    Isolated(turn)
}

#[cfg(test)]
pub struct Isolated(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

/// Where `file` lives.
pub fn path(file: File) -> PathBuf {
    config_dir().join(FOLDER).join(file.name())
}

/// Where `file` was under the app's old name.
pub fn legacy_path(file: File) -> PathBuf {
    config_dir().join(OLD_FOLDER).join(file.name())
}

/// The content of `file`; empty if absent. Falls back to what the app's old name left,
/// which is never rewritten.
pub fn load(file: File) -> String {
    fs::read_to_string(path(file)).or_else(|_| fs::read_to_string(legacy_path(file))).unwrap_or_default()
}

/// Writes `file` (atomically, its folder created if needed).
pub fn save(file: File, text: &str) -> io::Result<()> {
    let path = path(file);
    path.parent().map_or(Ok(()), fs::create_dir_all)?;
    vault::write(&path, text)
}

/// `save` for those who have no window to tell: a failure goes to the error output.
pub fn keep(file: File, text: &str) {
    if let Err(e) = save(file, text) {
        eprintln!("bref: {} ({}): {e}", tr("config not saved", "config non enregistrée"), path(file).display());
    }
}

/// (vault, open notes from most recent to oldest): one line each.
pub fn load_config() -> (Option<PathBuf>, Vec<PathBuf>) {
    let text = load(File::Config);
    let mut lines = text.lines().filter(|l| !l.is_empty()).map(PathBuf::from);
    (lines.next(), lines.collect())
}

pub fn save_config(vault: &Path, recent: &[PathBuf]) {
    let text: String = std::iter::once(vault)
        .chain(recent.iter().map(PathBuf::as_path))
        .map(|p| format!("{}\n", p.display()))
        .collect();
    keep(File::Config, &text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolation_moves_the_folders_and_never_uses_the_real_ones() {
        let root = Path::new("/tmp/bref-isolated-test");
        {
            let _guard = isolate(root);
            assert_eq!((config_dir(), home()), (root.join(".config"), Some(root.join(".config"))));
            assert_eq!(path(File::Keys), root.join(".config").join(FOLDER).join("keys"));
            assert_ne!(config_dir(), from_environment(), "never the user's folder");
        }
    }
}
