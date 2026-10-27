//! Tags, backlinks, front matter, trash, replace in vault, list pages, calendar, backup, search, daily note.

use super::*;

/// Returns the note open before the vault-wide replace, where the board chapter comes back.
pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) -> PathBuf {
    let (shell, root) = (&j.shell, &j.root);
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Tags: Ctrl+T lists the vault's tags; Right expands the first, Down
    // previews a note that carries it.
    fs::write(root.join("Tagué.md"), "# Tagué\n\n#alpha\n").unwrap();
    for _ in 0..2 {
        cx.executor().advance_clock(Duration::from_secs(3));
        cx.run_until_parked();
    }
    cx.simulate_keystrokes("secondary-t down right down");
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| s.nav.mode), Mode::Tags);
    assert!(vault::load_layout().starts_with("tags split"));
    let (tagged, previewed) = shell.read_with(cx, |s, _| {
        let tag = s.notes.iter().flat_map(|n| n.tags.iter()).min().unwrap().clone();
        let has = |n: &&Note| n.tags.contains(&tag);
        (s.notes.iter().filter(has).count(), s.notes.iter().filter(has).any(|n| Some(&n.path) == s.pane().path.as_ref()))
    });
    assert!(tagged > 0 && previewed);
    // A tag is not a file: Delete does nothing on it.
    cx.simulate_keystrokes("up delete");
    assert!(shell.read_with(cx, |s, _| s.palette.is_none() && !s.told(Tone::Failed, "")));

    // Backlinks: Ctrl+L lists the notes that lead to the open note (link with alias, link
    // in a table). Down previews one without the list changing; it follows a
    // removed link; Enter opens the note, and the list becomes its own.
    let was = shell.read_with(cx, |s, _| s.pane().path.clone()).unwrap();
    fs::write(root.join("Cible.md"), "# Cible\n").unwrap();
    fs::write(root.join("Source A.md"), "# Source A\n\nvoir [[Cible|la cible]] ici\n").unwrap();
    fs::write(root.join("Source B.md"), "# Source B\n\n| a | [[cible]] |\n").unwrap();
    j.settle(cx);
    shell.update(cx, |s, cx| s.open_note(&root.join("Cible.md"), cx));
    cx.simulate_keystrokes("secondary-l");
    cx.run_until_parked();
    let rows = |cx: &mut gpui::VisualTestContext| {
        shell.read_with(cx, |s, _| s.nav.rows.iter().map(|r| r.name.clone()).collect::<Vec<_>>())
    };
    assert_eq!(rows(cx), ["Source A", "Source B"]);
    assert!(vault::load_layout().starts_with("links split"));
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.pane().path.as_ref().is_some_and(|p| p.to_string_lossy().contains("Source"))));
    assert_eq!(rows(cx), ["Source A", "Source B"]);
    fs::write(root.join("Source B.md"), "# Source B\n").unwrap();
    j.settle(cx);
    assert_eq!(rows(cx), ["Source A"]);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.pane().path.as_deref() == Some(&*root.join("Source A.md"))) && rows(cx).is_empty());
    for name in ["Cible.md", "Source A.md", "Source B.md"] {
        fs::remove_file(root.join(name)).unwrap();
    }

    // Front matter: its lines are neither a heading nor a rule, the file
    // name follows the title placed after it, and neither typing, saving,
    // renaming nor link rewriting changes a byte of it.
    let meta = "---\ntags: [projet]\n# pas un titre\nvoir: \"[[Fiche]]\"\n---\n";
    fs::write(root.join("Fiche.md"), format!("{meta}# Fiche\n\ntexte\n")).unwrap();
    fs::write(root.join("Renvoi.md"), format!("{meta}# Renvoi\n\n[[Fiche]]\n")).unwrap();
    j.settle(cx);
    shell.update(cx, |s, cx| s.open_note(&root.join("Fiche.md"), cx));
    cx.run_until_parked();
    assert_eq!((j.kind(cx, 0), j.kind(cx, meta.find("# pas").unwrap())), (Some(markdown::Kind::Meta), Some(markdown::Kind::Meta)));
    assert_eq!(j.kind(cx, meta.len()), Some(markdown::Kind::Heading(1)));
    shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.jump(meta.len() + "# Fiche".len(), false, cx)));
    cx.simulate_input(" 2");
    j.settle(cx);
    assert_eq!(fs::read_to_string(root.join("Fiche 2.md")).unwrap(), format!("{meta}# Fiche 2\n\ntexte\n"));
    assert!(!root.join("Fiche.md").exists());
    // Links follow when leaving the renamed note.
    shell.update(cx, |s, cx| s.open_note(&root.join("Renvoi.md"), cx));
    j.settle(cx);
    assert_eq!(fs::read_to_string(root.join("Renvoi.md")).unwrap(), format!("{meta}# Renvoi\n\n[[Fiche 2]]\n"));
    // Its tags count as those of the text; its aliases are offered after `[[`, and a link written
    // with one of them opens the note instead of creating one.
    fs::write(root.join("Appli.md"), "---\ntags:\n  - enyaml\naliases: [Bref app]\n---\n# Appli\n").unwrap();
    j.settle(cx);
    assert!(shell.read_with(cx, |s, _| s.notes.iter().any(|n| n.name == "Appli" && n.tags[..] == ["enyaml"] && n.aliases[..] == ["Bref app"])));
    let count = shell.read_with(cx, |s, _| s.notes.len());
    cx.simulate_keystrokes("secondary-end enter");
    cx.simulate_input("[[bref a");
    cx.simulate_keystrokes("enter");
    assert!(text(cx).ends_with("[[Bref app]]"));
    shell.update_in(cx, |s, window, cx| s.open_wiki("bref APP", window, cx));
    j.settle(cx);
    assert!(shell.read_with(cx, |s, _| s.pane().path.as_deref() == Some(&*root.join("Appli.md")) && s.notes.len() == count));
    fs::remove_file(root.join("Appli.md")).unwrap();
    shell.update(cx, |s, cx| s.open_note(&root.join("Renvoi.md"), cx));
    shell.update(cx, |s, cx| s.open_note(&was, cx));
    j.settle(cx);
    for name in ["Fiche 2.md", "Renvoi.md"] {
        fs::remove_file(root.join(name)).unwrap();
    }
    j.settle(cx);

    // Trash: the palette lists what it holds; Enter puts the note back in the vault,
    // where it reappears among the notes.
    fs::write(root.join("Jetée.md"), "# Jetée\n").unwrap();
    j.settle(cx);
    vault::trash(root, &root.join("Jetée.md")).unwrap();
    j.settle(cx);
    assert!(shell.read_with(cx, |s, _| s.notes.iter().all(|n| n.name != "Jetée")));
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("corbeille");
    cx.simulate_keystrokes("down enter");
    cx.simulate_input("jet");
    cx.simulate_keystrokes("enter");
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "Restauré : Jetée.md")));
    j.settle(cx);
    assert!(root.join("Jetée.md").is_file() && !root.join(".trash/Jetée.md").exists());
    assert!(shell.read_with(cx, |s, _| s.notes.iter().any(|n| n.name == "Jetée") && s.toasts.is_empty()));
    fs::remove_file(root.join("Jetée.md")).unwrap();
    j.settle(cx);

    // Replace in the whole vault, from the search bar: the count first, then
    // the notes are rewritten; the palette undoes it all, except what changed since.
    let here = shell.read_with(cx, |s, _| s.pane().path.clone().unwrap());
    fs::write(root.join("Zoo A.md"), "# Zoo A\n\nle zèbre court").unwrap();
    fs::write(root.join("Zoo B.md"), "# Zoo B\n\nun zèbre, deux Zèbres").unwrap();
    j.settle(cx);
    shell.update(cx, |s, cx| s.open_note(&root.join("Zoo A.md"), cx));
    cx.simulate_keystrokes("secondary-r");
    cx.simulate_input("zèbre");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("okapi");
    cx.simulate_keystrokes("secondary-shift-enter");
    assert!(shell.read_with(cx, |s, _| s.palette.is_some()));
    assert_eq!(fs::read_to_string(root.join("Zoo B.md")).unwrap(), "# Zoo B\n\nun zèbre, deux Zèbres");
    cx.simulate_keystrokes("enter");
    assert_eq!(fs::read_to_string(root.join("Zoo B.md")).unwrap(), "# Zoo B\n\nun okapi, deux okapis");
    assert_eq!(text(cx), "# Zoo A\n\nle okapi court");
    assert!(shell.read_with(cx, |s, _| s.palette.is_none() && s.told(Tone::Done, "Remplacés : 3 · notes : 2")));
    cx.simulate_keystrokes("escape");
    fs::write(root.join("Zoo B.md"), "# Zoo B\n\nautre chose").unwrap();
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("annuler le remplacement");
    cx.simulate_keystrokes("down enter");
    assert_eq!(text(cx), "# Zoo A\n\nle zèbre court");
    assert_eq!(fs::read_to_string(root.join("Zoo B.md")).unwrap(), "# Zoo B\n\nautre chose");
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "Notes rétablies : 1 · modifiées depuis, laissées telles quelles : 1")));
    shell.update(cx, |s, cx| {
        s.toasts.clear();
        s.open_note(&here, cx)
    });
    fs::remove_file(root.join("Zoo A.md")).unwrap();
    fs::remove_file(root.join("Zoo B.md")).unwrap();
    j.settle(cx);

    // List page: a folder or a tag shown as a table of its notes, one column per key of
    // their front matter; a click on a column sorts, typing filters, a click on a
    // cell rewrites it in the note's front matter, the chosen note opens.
    fs::create_dir_all(root.join("Livres")).unwrap();
    fs::write(root.join("Livres/Dune.md"), "---\nauteur: Herbert\nnote: 9\n---\n# Dune\n").unwrap();
    fs::write(root.join("Livres/Emma.md"), "---\nauteur: Austen\nnote: 10\n---\n# Emma\n\n#classique\n").unwrap();
    j.settle(cx);
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::List, Some(root.join("Livres")), window, cx));
    let listed = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.listed(cx).map(|(keys, rows)| (keys, rows.into_iter().map(|(_, cells)| cells[0].clone()).collect::<Vec<_>>())));
    assert_eq!(listed(cx), Some((vec!["Note".to_string(), "auteur".into(), "note".into()], vec!["Dune".to_string(), "Emma".into()])));
    shell.update(cx, |s, cx| s.sort_listing(1, cx));
    assert_eq!(listed(cx).unwrap().1, ["Emma", "Dune"]);
    shell.update(cx, |s, cx| s.sort_listing(2, cx));
    shell.update(cx, |s, cx| s.sort_listing(2, cx));
    assert_eq!(listed(cx).unwrap().1, ["Emma", "Dune"]);
    // What is typed filters, in all columns or in the one named; Escape clears it.
    cx.run_until_parked();
    cx.simulate_input("auteur:her");
    assert_eq!(listed(cx).unwrap().1, ["Dune"]);
    cx.simulate_keystrokes("escape");
    cx.simulate_input("aus");
    assert_eq!(listed(cx).unwrap().1, ["Emma"]);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(listed(cx).unwrap().1, ["Emma", "Dune"]);
    // A click on a cell opens it; confirmed, only this key changes in the file.
    let cell = cx.debug_bounds("list-cell-1-2").unwrap().center();
    cx.simulate_click(cell, gpui::Modifiers::none());
    cx.simulate_keystrokes("backspace");
    cx.simulate_input("7.5");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(root.join("Livres/Dune.md")).unwrap(), "---\nauteur: Herbert\nnote: 7.5\n---\n# Dune\n");
    // Sorted by note in reverse: 10 before 7.5; Escape in a cell changes nothing.
    assert_eq!(listed(cx).unwrap().1, ["Emma", "Dune"]);
    shell.update_in(cx, |s, window, cx| s.list_write(root.join("Livres/Emma.md"), "auteur".into(), window, cx));
    cx.simulate_input(" Jane");
    cx.simulate_keystrokes("escape");
    assert!(fs::read_to_string(root.join("Livres/Emma.md")).unwrap().contains("auteur: Austen\n"));
    // A tag is listed like a folder.
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::List, Some(PathBuf::from("#classique")), window, cx));
    assert_eq!(listed(cx).unwrap().1, ["Emma"]);
    shell.update(cx, |s, cx| s.open_note(&root.join("Livres/Dune.md"), cx));
    assert!(shell.read_with(cx, |s, _| s.pane().listing.is_none()) && text(cx).ends_with("# Dune\n"));
    shell.update(cx, |s, cx| s.open_note(&here, cx));
    fs::remove_dir_all(root.join("Livres")).unwrap();
    j.settle(cx);

    // Calendar: Ctrl+Shift+J shows the month; arrows change the day, Page Down the
    // month, Enter opens that day's note, or creates it.
    let before = shell.read_with(cx, |s, _| (s.pane().path.clone(), s.nav.mode, s.nav.panel));
    cx.simulate_keystrokes("secondary-shift-j");
    assert!(shell.read_with(cx, |s, _| s.nav.mode == Mode::Calendar && s.nav.panel != Panel::Rail));
    shell.update(cx, |s, _| s.nav.day = (2026, 10, 30));
    cx.simulate_keystrokes("right down pagedown");
    assert_eq!(shell.read_with(cx, |s, _| s.nav.day), (2026, 12, 7));
    cx.simulate_keystrokes("pageup pageup up enter");
    assert_eq!(shell.read_with(cx, |s, _| s.nav.day), (2026, 9, 30));
    assert_eq!(text(cx), "# 2026-09-30\n\n");
    shell.update(cx, |s, cx| {
        (s.nav.mode, s.nav.panel) = (before.1, before.2);
        s.open_note(before.0.as_ref().unwrap(), cx);
    });
    // Once left, the daily note is saved like any note that has a title.
    j.settle(cx);
    fs::remove_file(root.join("2026-09-30.md")).unwrap();
    j.settle(cx);

    // Terminal: the palette offers it; tests open no window, hence the refusal.
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("terminal");
    cx.simulate_keystrokes("down enter");
    assert!(shell.read_with(cx, |s, _| s.palette.is_none() && s.told(Tone::Failed, "$TERMINAL")));
    shell.update(cx, |s, _| s.toasts.clear());

    // Capture shortcut: the palette offers it. Outside GNOME (and in tests) the
    // command to bind is copied, and the message says what to do with it.
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("capture rapide");
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.palette.is_none() && s.told(Tone::Done, "copiée")));
    assert!(cx.read_from_clipboard().and_then(|item| item.text()).is_some_and(|text| text.ends_with(" --capture")));
    shell.update(cx, |s, _| s.toasts.clear());

    // Backup: a dated archive of the whole vault in the chosen folder (the picker
    // does not exist in tests); inside the vault itself, it is refused.
    let out = root.with_file_name(format!("{}-sauvegardes", vault::stem(root)));
    fs::create_dir_all(&out).unwrap();
    shell.update(cx, |s, cx| s.backup_to(out.clone(), cx));
    cx.run_until_parked();
    let saved = fs::read_dir(&out).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect::<Vec<_>>();
    let toasts = shell.read_with(cx, |s, _| s.toasts.iter().map(|t| t.text.clone()).collect::<Vec<_>>());
    assert!(saved.len() == 1 && saved[0].ends_with(&format!(" {}.tar.gz", date_name(today()))), "{saved:?} {toasts:?}");
    assert!(shell.read_with(cx, |s, _| !s.told(Tone::Failed, "") && !s.told(Tone::Busy, "") && s.told(Tone::Done, "Sauvegarde enregistrée") && s.told(Tone::Done, &saved[0])));
    shell.update(cx, |s, cx| s.backup_to(root.join(".trash"), cx));
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Failed, "hors du coffre")));
    shell.update(cx, |s, _| s.toasts.clear());
    fs::remove_dir_all(&out).unwrap();

    // Search in the vault: Ctrl+Shift+F lists in the panel each match of the typed text, note by
    // note, ignoring case; Enter on a match opens its note, the passage selected.
    fs::write(root.join("Recette.md"), "# Recette\n\nfarine\n\ndu Quinoa rouge, quinoa\n").unwrap();
    fs::write(root.join("Liste.md"), "# Liste\n- quinoa\n").unwrap();
    j.settle(cx);
    cx.simulate_keystrokes("secondary-shift-f");
    cx.simulate_input("QUINOA");
    cx.run_until_parked();
    cx.simulate_keystrokes("down enter");
    j.settle(cx);
    let found = shell.read_with(cx, |s, cx| (s.pane().path.clone().unwrap(), s.pane().editor.read(cx).selected().to_lowercase()));
    assert!(found.0 == root.join("Recette.md") || found.0 == root.join("Liste.md"), "{found:?}");
    assert_eq!(found.1, "quinoa");
    cx.simulate_keystrokes("secondary-shift-f");
    cx.simulate_input("quinoa r");
    cx.run_until_parked();
    cx.simulate_keystrokes("down enter");
    j.settle(cx);
    assert_eq!(shell.read_with(cx, |s, cx| (s.pane().path.clone().unwrap(), s.pane().editor.read(cx).selected().to_string())), (root.join("Recette.md"), "Quinoa r".to_string()));
    shell.update(cx, |s, cx| s.open_note(&was, cx));
    for name in ["Recette.md", "Liste.md"] {
        fs::remove_file(root.join(name)).unwrap();
    }
    j.settle(cx);

    // Daily note: Ctrl+J creates it, named and titled with the local date; the second time
    // (here from the palette) the same note opens. `/date` writes the date.
    let day = date_name(today());
    cx.simulate_keystrokes("secondary-j");
    j.settle(cx);
    assert_eq!(fs::read_to_string(root.join(format!("{day}.md"))).unwrap(), format!("# {day}\n\n"));
    cx.simulate_input("vu le /date");
    cx.simulate_keystrokes("enter");
    assert_eq!(text(cx), format!("# {day}\n\nvu le {day}"));
    j.settle(cx);
    shell.update(cx, |s, cx| s.open_note(&was, cx));
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("note du jour");
    cx.simulate_keystrokes("down enter");
    j.settle(cx);
    assert_eq!(text(cx), format!("# {day}\n\nvu le {day}"));
    assert!(!root.join(format!("{day} 2.md")).exists());
    shell.update(cx, |s, cx| s.open_note(&was, cx));
    fs::remove_file(root.join(format!("{day}.md"))).unwrap();
    j.settle(cx);
    shell.update(cx, |s, cx| s.open_note(&was, cx));
    j.settle(cx);
    cx.simulate_keystrokes("secondary-t");
    cx.run_until_parked();
    here
}
