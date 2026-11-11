//! The vault health page: opened from the palette, it lists what is wrong in the vault by kind; a broken
//! link is relinked to the close name (in the open note, as one undo step) or loses its brackets, "Fix all"
//! relinks the others, an unused picture and a conflict copy go to the trash. The palette's command wears
//! a dot when new broken links appear.

use super::*;
use crate::logic::health::Kind;

type Cx = gpui::VisualTestContext;

fn click(cx: &mut Cx, name: &str) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let at = cx.debug_bounds(leak(name.to_string())).unwrap_or_else(|| panic!("{name}")).center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
}

/// How many findings of each kind the page shows.
fn counts(shell: &Entity<Shell>, cx: &mut Cx) -> Vec<(Kind, usize)> {
    cx.run_until_parked();
    shell.read_with(cx, |s, _| s.health_found().map(crate::logic::health::counts).unwrap_or_default())
}

fn broken(shell: &Entity<Shell>, cx: &mut Cx) -> usize {
    counts(shell, cx).iter().find(|(kind, _)| *kind == Kind::BrokenLink).map_or(0, |(_, n)| *n)
}

#[gpui::test]
fn the_vault_health_page(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "vault-health");
    fs::write(root.join("Recettes.md"), "# Recettes\n\nTarte\n").unwrap();
    fs::write(root.join("Lundi.md"), "# Lundi\n\n[[Recetes]] puis [[Absente|à voir]]\n").unwrap();
    fs::write(root.join("Note.md"), "# Note\n\n[[Lundi]]\n").unwrap();
    fs::write(root.join("Note (conflict 2026-10-09 pc).md"), "# Note\n\nautre version\n").unwrap();
    fs::create_dir(root.join("img")).unwrap();
    fs::write(root.join("img/oubliée.png"), b"png").unwrap();
    shell.update(cx, |s, cx| {
        s.rescan(&root, cx);
        s.open_note(&root.join("Lundi.md"), cx);
    });
    cx.run_until_parked();
    // The first look at the palette takes the broken links there as seen: no dot. A new one lights it.
    assert!(!shell.update(cx, |s, _| s.health_dot()));
    fs::write(root.join("Mardi.md"), "# Mardi\n\n[[Recetes]] encore\n[[recetes#Tarte]]\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.run_until_parked();
    assert!(shell.update(cx, |s, _| s.health_dot()));

    // Palette › santé: the page opens in a tab of its own, beside the note, and lists by kind.
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("santé");
    // « Créer « santé » » comes first, as for any name no note has.
    cx.simulate_keystrokes("down enter");
    let (tabs, shown) = shell.read_with(cx, |s, cx| (s.panes.len(), s.tab_name(s.active, cx)));
    assert_eq!((tabs, shown.as_str()), (2, "Santé du coffre"));
    assert_eq!(
        counts(&shell, cx),
        [(Kind::BrokenLink, 4), (Kind::Conflict, 1), (Kind::Unused, 1), (Kind::Orphan, 3)]
    );
    // The page was looked at: the dot is out.
    assert!(!shell.update(cx, |s, _| s.health_dot()));

    // Lundi's `[[Recetes]]`: relinked to « Recettes », the only close name. Lundi is open in the other tab:
    // the change goes through its editor, as one undo step, and is saved.
    click(cx, "health-fix-BrokenLink-0-2");
    assert_eq!(fs::read_to_string(root.join("Lundi.md")).unwrap(), "# Lundi\n\n[[Recettes]] puis [[Absente|à voir]]\n");
    let lundi = shell.read_with(cx, |s, cx| s.panes.iter().find(|p| p.path.as_deref() == Some(&root.join("Lundi.md"))).map(|p| p.editor.read(cx).text().to_string()));
    assert_eq!(lundi.as_deref(), Some("# Lundi\n\n[[Recettes]] puis [[Absente|à voir]]\n"));
    assert_eq!(broken(&shell, cx), 3);

    // `[[Absente|à voir]]` has no close name: its brackets go, what it showed stays.
    click(cx, "health-fix-BrokenLink-0-2");
    assert_eq!(fs::read_to_string(root.join("Lundi.md")).unwrap(), "# Lundi\n\n[[Recettes]] puis à voir\n");
    assert_eq!(broken(&shell, cx), 2);

    // "Fix all" relinks Mardi's two spellings of `Recetes` at once, anchor kept; Mardi was not open: written on disk.
    click(cx, "health-fix-all");
    assert_eq!(fs::read_to_string(root.join("Mardi.md")).unwrap(), "# Mardi\n\n[[Recettes]] encore\n[[Recettes#Tarte]]\n");
    assert_eq!(broken(&shell, cx), 0);

    // The conflict copy: keep the original, the copy goes to the trash. The unused picture too.
    click(cx, "health-fix-Conflict-0-2");
    assert!(root.join(".trash/Note (conflict 2026-10-09 pc).md").is_file() && root.join("Note.md").is_file());
    click(cx, "health-fix-Unused-0-1");
    assert!(root.join(".trash/oubliée.png").is_file() && !root.join("img/oubliée.png").exists());
    // Recettes, then Mardi, are linked now; the copy is gone: nothing is left to fix.
    assert_eq!(counts(&shell, cx), []);
    shell.read_with(cx, |s, _| assert!(s.health_found().is_some_and(|f| f.is_empty()), "{:?}", s.health_found()));
    // Each fix of the open note is one undo step there: undo brings the brackets back, and only them.
    shell.update_in(cx, |s, window, cx| {
        let lundi = s.panes.iter().position(|p| p.path.as_deref() == Some(&root.join("Lundi.md"))).unwrap();
        s.focus_pane(lundi, window, cx);
    });
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string()), "# Lundi\n\n[[Recettes]] puis [[Absente|à voir]]\n");
}
