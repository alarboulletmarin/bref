//! Opening the system terminal in a folder.

use std::{env, path::Path};

/// The commands to try, in order, to open a terminal in `dir` under the system
/// `os` (`std::env::consts::OS`): the program and its arguments. `preferred` is the terminal
/// the user names in `$TERMINAL`.
pub fn terminals(os: &str, dir: &str, preferred: Option<&str>) -> Vec<(String, Vec<String>)> {
    let command = |program: &str, args: &[&str]| (program.to_string(), args.iter().map(|a| a.to_string()).collect());
    match os {
        "macos" => vec![command("open", &["-a", "Terminal", dir])],
        // Windows Terminal if installed, otherwise the classic console.
        "windows" => vec![command("wt", &["-d", dir]), command("cmd", &["/c", "start", "cmd"])],
        // Linux has no single answer. Those that merely notify an already running instance
        // ignore the current folder: it is given to them as an argument.
        _ => {
            let mut all: Vec<_> = preferred.iter().map(|p| command(p, &[])).collect();
            all.extend([
                command("xdg-terminal-exec", &[]),
                command("x-terminal-emulator", &[]),
                command("gnome-terminal", &["--working-directory", dir]),
                command("ptyxis", &["--new-window", "-d", dir]),
                command("kgx", &["--working-directory", dir]),
                command("konsole", &["--workdir", dir]),
                command("kitty", &[]),
                command("alacritty", &[]),
                command("foot", &[]),
                command("wezterm", &["start", "--cwd", dir]),
                command("xterm", &[]),
            ]);
            all
        }
    }
}

/// Opens the system terminal in `dir`: the first of `terminals` that starts. False if
/// none is installed. Tests open nothing.
pub fn open_terminal(dir: &Path) -> bool {
    let preferred = env::var("TERMINAL").ok().filter(|t| !t.trim().is_empty());
    let found = terminals(env::consts::OS, &dir.to_string_lossy(), preferred.as_deref());
    !cfg!(test)
        && found.into_iter().any(|(program, args)| {
            // No console for the launcher (`cmd /c start`); the terminal opens its own.
            let child = crate::platform::command(&program).args(args).current_dir(dir).stdin(std::process::Stdio::null()).spawn();
            // Once the terminal is closed, its process is reaped: no zombie while Bref runs.
            child.map(|mut child| std::thread::spawn(move || child.wait())).is_ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_terminal_of_the_system() {
        let programs = |os, preferred| terminals(os, "/coffre", preferred).into_iter().map(|(p, _)| p).collect::<Vec<_>>();
        assert_eq!(terminals("macos", "/coffre", None), [("open".to_string(), vec!["-a".to_string(), "Terminal".into(), "/coffre".into()])]);
        assert_eq!(programs("windows", Some("kitty")), ["wt", "cmd"]);
        // `$TERMINAL` comes first; without it, the desktop's choice first.
        assert_eq!(programs("linux", Some("foot"))[..2], ["foot", "xdg-terminal-exec"]);
        assert_eq!(programs("linux", None)[0], "xdg-terminal-exec");
        let gnome = terminals("linux", "/coffre", None).into_iter().find(|(p, _)| p == "gnome-terminal").unwrap();
        assert_eq!(gnome.1, ["--working-directory", "/coffre"]);
    }
}
