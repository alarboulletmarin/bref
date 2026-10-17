//! Git sync with a bare server and another machine, and cloning a vault.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let (shell, root) = (&j.shell, &j.root);
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Synchronisation par git : un serveur (dépôt nu) et une autre machine, dans le dossier
    // de test. Le coffre de ce parcours-ci est la première machine.
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
    // Un coffre sans git : « Synchroniser » demande l'adresse du dépôt, puis le relie.
    cx.simulate_keystrokes("secondary-shift-s");
    assert!(shell.read_with(cx, |s, _| s.palette.is_some()), "l'adresse est demandée");
    cx.simulate_keystrokes("escape");
    shell.update(cx, |s, cx| s.git_connect(url.clone(), cx));
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "relié")), "{:?}", shell.read_with(cx, |s, _| s.toasts.iter().map(|t| t.text.clone()).collect::<Vec<_>>()));
    // Une autre machine clone le coffre, écrit une note et change le carnet, puis envoie.
    git::clone(&url, &there).unwrap();
    // Elle reçoit d'abord ce que le serveur a, comme le ferait sa propre synchronisation,
    // puis écrit : ses changements partent de la dernière version commune.
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
    // Ici, on tape, puis une seule action : tout est commité, reçu, fusionné, envoyé, et la
    // note ouverte montre ce qui a été reçu sans perdre ce qui vient d'être tapé.
    cx.simulate_keystrokes("secondary-home down down");
    cx.simulate_input("zéro");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("secondary-shift-s");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "1 envoyé") && s.told(Tone::Done, "2 reçus")), "{:?}", shell.read_with(cx, |s, _| s.toasts.iter().map(|t| t.text.clone()).collect::<Vec<_>>()));
    assert_eq!(text(cx), "# Carnet\n\nzéro\nun\ndeux\ntrois\n");
    assert!(shell.read_with(cx, |s, _| s.notes.iter().any(|note| note.name == "Venue d'ailleurs")));
    // Rien de nouveau : la commande le dit.
    cx.simulate_keystrokes("secondary-shift-s");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "à jour")));
    // La même ligne changée des deux côtés : la note garde ce qu'on y a écrit, la version
    // d'ailleurs devient une note à côté, et aucun marqueur n'entre dans le texte.
    other(&|| fs::write(there.join("Carnet.md"), "# Carnet\n\nzéro\nUN (ailleurs)\ndeux\ntrois\n").unwrap());
    cx.simulate_keystrokes("secondary-home down down down end");
    cx.simulate_input(" (ici)");
    cx.simulate_keystrokes("secondary-shift-s");
    cx.run_until_parked();
    assert_eq!(text(cx), "# Carnet\n\nzéro\nun (ici)\ndeux\ntrois\n");
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "conflit")));
    let copy = fs::read_dir(&here).unwrap().flatten().map(|e| e.path()).find(|p| p.file_name().unwrap().to_string_lossy().starts_with("Carnet (conflict ")).unwrap();
    assert_eq!(fs::read_to_string(&copy).unwrap(), "# Carnet\n\nzéro\nUN (ailleurs)\ndeux\ntrois\n");
    assert!(shell.read_with(cx, |s, _| s.notes.iter().any(|note| note.path == copy)), "la copie est dans le coffre");
    // L'autre machine reçoit les deux versions.
    other(&|| ());
    assert_eq!(fs::read_to_string(there.join("Carnet.md")).unwrap(), "# Carnet\n\nzéro\nun (ici)\ndeux\ntrois\n");
    assert!(there.join(copy.file_name().unwrap()).is_file());
    // Un serveur injoignable : un message clair, et le coffre reste tel qu'il était.
    git(&here, &["remote", "set-url", "origin", lab.join("nulle-part.git").to_str().unwrap()]);
    cx.simulate_input("!");
    cx.simulate_keystrokes("secondary-shift-s");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Failed, "Synchronisation impossible")));
    assert_eq!(text(cx), "# Carnet\n\nzéro\nun (ici)!\ndeux\ntrois\n");
    assert!(!shell.read_with(cx, |s, _| s.syncing));
    // Cloner un coffre : le dossier est créé, et devient le coffre ouvert.
    let shelf = lab.join("clones");
    fs::create_dir_all(&shelf).unwrap();
    shell.update_in(cx, |s, window, cx| s.clone_to(url.clone(), shelf.clone(), window, cx));
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| s.vault.clone()), Some(shelf.join("remote")));
    assert!(shelf.join("remote/Venue d'ailleurs.md").is_file());
}
