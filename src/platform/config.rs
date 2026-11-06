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
/// move it under a temporary folder (`isolated_config`), so they read it each time.
pub fn config_dir() -> PathBuf {
    #[cfg(test)]
    return from_environment();
    #[cfg(not(test))]
    {
        static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        DIR.get_or_init(from_environment).clone()
    }
}

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
