//! Several vaults: Bref remembers those it opened; Ctrl+O lists them and switches, each vault getting back its
//! tabs and recent notes; a vault takes a short name and a colour (the band of the rail), leaves the list
//! without its folder being touched, moves in it, and, its folder renamed, is found again with its session.

use super::*;
use crate::logic::vaults::read;

type Cx = gpui::VisualTestContext;

/// The tabs of the window, by name, and the vault open.
fn tabs(shell: &Entity<Shell>, cx: &mut Cx) -> (Option<PathBuf>, Vec<String>) {
    cx.run_until_parked();
    shell.read_with(cx, |s, cx| (s.vault.clone(), s.groups.iter().flat_map(|g| g.tabs.clone()).map(|t| s.tab_name(t, cx)).collect()))
}

/// What the switcher lists, row by row.
fn rows(shell: &Entity<Shell>, cx: &mut Cx) -> Vec<String> {
    cx.run_until_parked();
    shell.read_with(cx, |s, cx| {
        let palette = s.palette.as_ref().expect("the switcher").read(cx);
        (0..palette.listed()).filter_map(|i| palette.choice_at(i)).collect()
    })
}

/// Opens a file as a click would, then draws: the history records what is shown at each frame.
fn open(shell: &Entity<Shell>, cx: &mut Cx, path: &Path) {
    shell.update_in(cx, |s, window, cx| s.open(path, window, cx));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn labels(shell: &Entity<Shell>, cx: &mut Cx) -> Vec<String> {
    shell.read_with(cx, |s, _| s.vaults.iter().map(|k| k.label()).collect())
}

#[gpui::test]
fn several_vaults(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "vault-switch");
    let other = root.with_file_name(format!("{}-Perso", root.file_name().unwrap().to_string_lossy()));
    let _ = fs::remove_dir_all(&other);
    fs::create_dir_all(&other).unwrap();
    fs::write(root.join("Réunion.md"), "# Réunion\n").unwrap();
    fs::write(root.join("Budget.md"), "# Budget\n").unwrap();
    fs::write(other.join("Vacances.md"), "# Vacances\n").unwrap();
    // The work vault: two tabs, and the second one's note went back to the first.
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    for name in ["Réunion.md", "Budget.md", "Réunion.md"] {
        open(&shell, cx, &root.join(name));
    }
    assert_eq!(tabs(&shell, cx), (Some(root.clone()), vec!["Réunion".to_string(), "Budget".to_string()]));
    assert!(shell.read_with(cx, |s, _| s.history.can_back()));
    // Its panel shows the tags.
    shell.update_in(cx, |s, window, cx| s.show_nav(nav::Mode::Tags, false, window, cx));

    // Another folder (the dialog's answer): a new vault, on a new note, nothing of the first one's history.
    shell.update_in(cx, |s, window, cx| s.switch_to(other.clone(), window, cx));
    cx.update(|window, _| window.refresh());
    assert_eq!(tabs(&shell, cx).0, Some(other.clone()));
    assert!(!shell.read_with(cx, |s, _| s.history.can_back()));
    open(&shell, cx, &other.join("Vacances.md"));
    shell.update_in(cx, |s, window, cx| s.show_nav(nav::Mode::Tree, false, window, cx));

    // Ctrl+O: both vaults, the open one marked and selected, then the folder dialog's row.
    let work = root.file_name().unwrap().to_string_lossy().into_owned();
    let perso = other.file_name().unwrap().to_string_lossy().into_owned();
    cx.simulate_keystrokes("secondary-o");
    assert_eq!(rows(&shell, cx), [work.clone(), format!("● {perso}"), "Ouvrir un autre dossier…".to_string()]);
    // Up, Enter: the work vault, its two tabs, the one shown, its recent notes and its back and forward.
    cx.simulate_keystrokes("up enter");
    assert_eq!(tabs(&shell, cx), (Some(root.clone()), vec!["Réunion".to_string(), "Budget".to_string()]));
    let (shown, recent, back) = shell.read_with(cx, |s, _| (s.pane().path.clone(), s.recent.first().cloned(), s.history.can_back()));
    assert_eq!((shown, recent, back), (Some(root.join("Réunion.md")), Some(root.join("Réunion.md")), true));
    assert_eq!(shell.read_with(cx, |s, _| s.nav.mode()), nav::Mode::Tags);
    // And back: the personal vault on its note.
    cx.simulate_keystrokes("secondary-o down enter");
    assert_eq!(tabs(&shell, cx), (Some(other.clone()), vec!["Vacances".to_string()]));
    assert_eq!(shell.read_with(cx, |s, _| s.nav.mode()), nav::Mode::Tree);

    // A short name and a colour: the band at the top of the rail shows it.
    assert!(cx.debug_bounds("vault-band").is_none());
    shell.update_in(cx, |s, window, cx| s.name_vault(window, cx));
    cx.simulate_input("Perso");
    cx.simulate_keystrokes("enter");
    shell.update_in(cx, |s, window, cx| s.color_vault(window, cx));
    // « Aucune couleur », then Jaune, then Vert.
    cx.simulate_keystrokes("down down enter");
    let known = shell.read_with(cx, |s, _| s.known_vault().cloned()).unwrap();
    assert_eq!((known.label(), known.color), ("Perso".to_string(), Some(1)));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("vault-band").is_some());
    // The list is on disk, name and colour included.
    let saved = read(&config::load(config::File::Vaults));
    assert_eq!(saved.iter().map(|k| (k.label(), k.color)).collect::<Vec<_>>(), [(work.clone(), None), ("Perso".to_string(), Some(1))]);

    // Alt+Up moves the open vault to the top; Shift+Delete takes the work vault off the list, not off the disk.
    cx.simulate_keystrokes("secondary-o alt-up");
    assert_eq!(rows(&shell, cx), ["● Perso".to_string(), work.clone(), "Ouvrir un autre dossier…".to_string()]);
    cx.simulate_keystrokes("down shift-delete");
    assert_eq!(rows(&shell, cx), ["● Perso".to_string(), "Ouvrir un autre dossier…".to_string()]);
    // The open vault and the folder dialog's row cannot be taken off.
    cx.simulate_keystrokes("up shift-delete down shift-delete escape");
    assert_eq!(labels(&shell, cx), ["Perso"]);
    assert!(root.join("Réunion.md").is_file());

    // The work vault opened again comes back at the end of the list, as new: its session left with it.
    shell.update_in(cx, |s, window, cx| s.switch_to(root.clone(), window, cx));
    assert_eq!(labels(&shell, cx), ["Perso".to_string(), work.clone()]);
    assert_eq!(tabs(&shell, cx).1.len(), 1);

    // The personal vault's folder is renamed elsewhere: listed, faded, found again with its name, colour and tab.
    let moved = other.with_file_name(format!("{perso}-moved"));
    let _ = fs::remove_dir_all(&moved);
    fs::rename(&other, &moved).unwrap();
    cx.simulate_keystrokes("secondary-o");
    let faded = shell.read_with(cx, |s, cx| s.palette.as_ref().map(|p| p.read(cx).faded_at(0)));
    assert_eq!(faded, Some(true));
    cx.simulate_keystrokes("escape");
    shell.update_in(cx, |s, window, cx| s.found_vault(0, moved.clone(), window, cx));
    assert_eq!(tabs(&shell, cx), (Some(moved.clone()), vec!["Vacances".to_string()]));
    let known = shell.read_with(cx, |s, _| s.known_vault().cloned()).unwrap();
    assert_eq!((known.label(), known.color, known.path), ("Perso".to_string(), Some(1), moved.clone()));
    let _ = fs::remove_dir_all(&moved);
}
