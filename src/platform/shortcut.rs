//! The desktop shortcut for quick capture (GNOME's custom shortcuts).

use std::{env, path::Path};

use crate::lang::tr;

/// Keys offered for capture; free on a stock GNOME.
const CAPTURE_KEYS: &str = "<Super><Shift>n";
const GNOME_KEYS: &str = "org.gnome.settings-daemon.plugins.media-keys";
/// A path of our own in GNOME's custom shortcuts: asking again doubles nothing.
const GNOME_PATH: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/bref-capture/";

/// What asking for a capture shortcut gave.
#[derive(Debug, PartialEq)]
pub enum Shortcut {
    /// Set in the desktop settings: the keys, as read.
    Bound(String),
    /// Set without keys, the offered ones being taken: here they are, to choose in the settings.
    Unbound(String),
    /// This desktop cannot be set from here: the command to bind yourself.
    Manual(String),
}

/// Binds `exe --capture` to a desktop shortcut, never taking keys already
/// in use. Under GNOME it is done from here; elsewhere the command is returned, to bind by hand.
/// Blocking (a few `gsettings` calls): run off the UI thread. Tests
/// touch no setting.
pub fn capture_shortcut(exe: &Path) -> Shortcut {
    let exe = exe.to_string_lossy();
    let command = if exe.contains(' ') { format!("'{exe}' --capture") } else { format!("{exe} --capture") };
    let gnome = !cfg!(test) && env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktop| desktop.contains("GNOME"));
    gnome.then(|| gnome_shortcut(&command)).flatten().unwrap_or(Shortcut::Manual(command))
}

/// What `gsettings` answers, if it is there and accepts.
fn gsettings(args: &[&str]) -> Option<String> {
    let out = crate::platform::command("gsettings").args(args).stdin(std::process::Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// GNOME's custom shortcut "Bref: capture": created with `CAPTURE_KEYS` if nothing
/// uses them, otherwise without keys. If it already exists, only its command is updated (the
/// binary may have moved): the keys the user gave it stay.
fn gnome_shortcut(command: &str) -> Option<Shortcut> {
    let entry = format!("{GNOME_KEYS}.custom-keybinding:{GNOME_PATH}");
    let list = gsettings(&["get", GNOME_KEYS, "custom-keybindings"])?;
    let quoted = |text: &str| format!("'{}'", text.replace('\'', "\\'"));
    let Some(longer) = with_keybinding(&list, GNOME_PATH) else {
        gsettings(&["set", &entry, "command", &quoted(command)])?;
        let keys = gsettings(&["get", &entry, "binding"])?;
        let keys = keys.trim_matches('\'');
        return Some(if keys.is_empty() { Shortcut::Unbound(spell_keys(CAPTURE_KEYS)) } else { Shortcut::Bound(spell_keys(keys)) });
    };
    // Taken: by a system shortcut, or by another custom shortcut.
    let wanted = format!("'{}'", CAPTURE_KEYS.to_lowercase());
    let others = list.split('\'').skip(1).step_by(2).filter_map(|path| gsettings(&["get", &format!("{GNOME_KEYS}.custom-keybinding:{path}"), "binding"]));
    let taken = gsettings(&["list-recursively"]).is_some_and(|all| all.to_lowercase().contains(&wanted))
        || others.into_iter().any(|keys| keys.to_lowercase() == wanted);
    gsettings(&["set", &entry, "name", &quoted(tr("Bref: capture", "Bref : capture"))])?;
    gsettings(&["set", &entry, "command", &quoted(command)])?;
    if !taken {
        gsettings(&["set", &entry, "binding", &quoted(CAPTURE_KEYS)])?;
    }
    gsettings(&["set", GNOME_KEYS, "custom-keybindings", &longer])?;
    Some(if taken { Shortcut::Unbound(spell_keys(CAPTURE_KEYS)) } else { Shortcut::Bound(spell_keys(CAPTURE_KEYS)) })
}

/// The `['a', 'b']` list from `gsettings` (or `@as []`, empty) with `path` added; `None` if it
/// is already there.
fn with_keybinding(list: &str, path: &str) -> Option<String> {
    let mut paths: Vec<&str> = list.split('\'').skip(1).step_by(2).collect();
    if paths.contains(&path) {
        return None;
    }
    paths.push(path);
    Some(format!("[{}]", paths.iter().map(|p| format!("'{p}'")).collect::<Vec<_>>().join(", ")))
}

/// `<Super><Shift>n`, as GNOME writes it, as read: `Super+Shift+N`.
fn spell_keys(keys: &str) -> String {
    let parts = keys.split(['<', '>']).filter(|part| !part.is_empty()).map(|part| match part {
        "Shift" => tr("Shift", "Maj").to_string(),
        "Primary" | "Control" => "Ctrl".to_string(),
        _ => {
            let mut chars = part.chars();
            chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
        }
    });
    parts.collect::<Vec<_>>().join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn gnome_shortcut_list_gains_the_capture_once() {
        let path = "/org/gnome/x/bref-capture/";
        assert_eq!(with_keybinding("@as []", path).as_deref(), Some("['/org/gnome/x/bref-capture/']"));
        assert_eq!(with_keybinding("['/org/gnome/x/custom0/', '/org/gnome/x/custom1/']", path).as_deref(), Some("['/org/gnome/x/custom0/', '/org/gnome/x/custom1/', '/org/gnome/x/bref-capture/']"));
        // Already there: nothing to add, the request can repeat without doubling anything.
        assert_eq!(with_keybinding("['/org/gnome/x/custom0/', '/org/gnome/x/bref-capture/']", path), None);
    }

    #[test]
    fn gnome_keys_are_spelled_like_the_others() {
        assert_eq!(spell_keys("<Super><Shift>n"), "Super+Maj+N");
        assert_eq!(spell_keys("<Primary><Alt>space"), "Ctrl+Alt+Space");
        assert_eq!(spell_keys("F9"), "F9");
    }

    /// The real `gsettings`, on throwaway settings (key-value file in a temporary
    /// folder): `cargo test -- --ignored gnome_shortcut_is_registered`, on a GNOME machine.
    #[test]
    #[ignore]
    fn gnome_shortcut_is_registered() {
        let dir = std::env::temp_dir().join(format!("bref-gsettings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // The child process `gsettings` reads these two variables: only the environment reaches it, so this
        // ignored test (run by hand, alone) keeps `set_var`.
        unsafe {
            std::env::set_var("GSETTINGS_BACKEND", "keyfile");
            std::env::set_var("XDG_CONFIG_HOME", &dir);
        }
        assert_eq!(gnome_shortcut("/usr/bin/bref --capture"), Some(Shortcut::Bound("Super+Maj+N".into())));
        let saved = fs::read_to_string(dir.join("glib-2.0/settings/keyfile")).unwrap();
        assert!(saved.contains("command='/usr/bin/bref --capture'") && saved.contains("binding='<Super><Shift>n'"), "{saved}");
        // Asked again after the binary moved: the command follows, the chosen key stays.
        gsettings(&["set", &format!("{GNOME_KEYS}.custom-keybinding:{GNOME_PATH}"), "binding", "<Super>F9"]).unwrap();
        assert_eq!(gnome_shortcut("/opt/bref --capture"), Some(Shortcut::Bound("Super+F9".into())));
        let saved = fs::read_to_string(dir.join("glib-2.0/settings/keyfile")).unwrap();
        assert!(saved.contains("command='/opt/bref --capture'") && saved.matches("bref-capture").count() == 2, "{saved}");
        let _ = fs::remove_dir_all(&dir);
    }
}
