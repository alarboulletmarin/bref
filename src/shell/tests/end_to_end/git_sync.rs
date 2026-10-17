//! Git sync with a bare server and another machine, and cloning a vault.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let (shell, root) = (&j.shell, &j.root);
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
        // Sync through git: a server (bare repository) and another machine, in the test
        // folder. This run's vault is the first machine.
    let lab = root.join("sync");
    let (server, here, there) = (lab.join("remote.git"), lab.join("ici"), lab.join("ailleurs"));
    fs::create_dir_all(&server).unwrap();
    fs::create_dir_all(&here).unwrap();
    let git = |dir: &Path, args: &[&str]| assert!(std::process::Command::new("git").current_dir(dir).args(args).output().unwrap().status.success(), "{args:?}");
    git(&server, &["init", "-q", "--bare", "-b", "main"]);
    let url = server.to_str().unwrap().to_string();
    fs::write(here.join("Carnet.md"), "# Carnet\n\nun\ndeux\n").unwrap();
    shell.update_in(cx, |s, window, cx| s.set_vault(here.clone(), window, cx));
    cx.run_until_parked();
    shell.update(cx, |s, cx| s.open_note(&here.join("Carnet.md"), cx));
        // A vault without git: « Synchroniser » asks for the repository address, then links it.
    cx.simulate_keystrokes("secondary-shift-s");
    assert!(shell.read_with(cx, |s, _| s.palette.is_some()), "the address is asked for");
    cx.simulate_keystrokes("escape");
    shell.update(cx, |s, cx| s.git_connect(url.clone(), cx));
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "relié")), "{:?}", shell.read_with(cx, |s, _| s.toasts.iter().map(|t| t.text.clone()).collect::<Vec<_>>()));
        // Another machine clones the vault, writes a note and changes the notebook, then pushes.
    git::clone(&url, &there).unwrap();
        // It first receives what the server has, as its own sync would,
        // then writes: its changes start from the last common version.
    let other = |write: &dyn Fn()| {
        git::fetch(&there).unwrap();
        git::merge(&there, "2026-10-09", "ailleurs").unwrap();
        write();
        git::save(&there).unwrap();
        git::fetch(&there).unwrap();
        git::merge(&there, "2026-10-09", "ailleurs").unwrap();
        assert_eq!(git::push(&there), Ok(git::Pushed::Done));
    };
    other(&|| {
        fs::write(there.join("Venue d'ailleurs.md"), "# Venue d'ailleurs\n").unwrap();
        fs::write(there.join("Carnet.md"), "# Carnet\n\nun\ndeux\ntrois\n").unwrap();
    });
        // Here, we type, then a single action: everything is committed, received, merged, pushed, and the
        // open note shows what was received without losing what was just typed.
    cx.simulate_keystrokes("secondary-home down down");
    cx.simulate_input("zéro");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("secondary-shift-s");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "1 envoyé") && s.told(Tone::Done, "2 reçus")), "{:?}", shell.read_with(cx, |s, _| s.toasts.iter().map(|t| t.text.clone()).collect::<Vec<_>>()));
    assert_eq!(text(cx), "# Carnet\n\nzéro\nun\ndeux\ntrois\n");
    assert!(shell.read_with(cx, |s, _| s.notes.iter().any(|note| note.name == "Venue d'ailleurs")));
        // Nothing new: the command says so.
    cx.simulate_keystrokes("secondary-shift-s");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "à jour")));
        // The same line changed on both sides: the note keeps what was written in it, the version
        // from elsewhere becomes a note beside it, and no marker enters the text.
    other(&|| fs::write(there.join("Carnet.md"), "# Carnet\n\nzéro\nUN (ailleurs)\ndeux\ntrois\n").unwrap());
    cx.simulate_keystrokes("secondary-home down down down end");
    cx.simulate_input(" (ici)");
    cx.simulate_keystrokes("secondary-shift-s");
    cx.run_until_parked();
    assert_eq!(text(cx), "# Carnet\n\nzéro\nun (ici)\ndeux\ntrois\n");
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "conflit")));
    let copy = fs::read_dir(&here).unwrap().flatten().map(|e| e.path()).find(|p| p.file_name().unwrap().to_string_lossy().starts_with("Carnet (conflict ")).unwrap();
    assert_eq!(fs::read_to_string(&copy).unwrap(), "# Carnet\n\nzéro\nUN (ailleurs)\ndeux\ntrois\n");
    assert!(shell.read_with(cx, |s, _| s.notes.iter().any(|note| note.path == copy)), "the copy is in the vault");
        // The other machine receives both versions.
    other(&|| ());
    assert_eq!(fs::read_to_string(there.join("Carnet.md")).unwrap(), "# Carnet\n\nzéro\nun (ici)\ndeux\ntrois\n");
    assert!(there.join(copy.file_name().unwrap()).is_file());
        // An unreachable server: a clear message, and the vault stays as it was.
    git(&here, &["remote", "set-url", "origin", lab.join("nulle-part.git").to_str().unwrap()]);
    cx.simulate_input("!");
    cx.simulate_keystrokes("secondary-shift-s");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Failed, "Synchronisation impossible")));
    assert_eq!(text(cx), "# Carnet\n\nzéro\nun (ici)!\ndeux\ntrois\n");
    assert!(!shell.read_with(cx, |s, _| s.syncing));
        // Cloning a vault: the folder is created, and becomes the open vault.
    let shelf = lab.join("clones");
    fs::create_dir_all(&shelf).unwrap();
    shell.update_in(cx, |s, window, cx| s.clone_to(url.clone(), shelf.clone(), window, cx));
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| s.vault.clone()), Some(shelf.join("remote")));
    assert!(shelf.join("remote/Venue d'ailleurs.md").is_file());
}
