//! The window around the notes: badge, format bar, saved shortcuts and panes, first launch, quick capture, empty page.

use super::*;

#[test]
fn badge_shows_the_count_up_to_nine() {
    assert_eq!(["1", "9", "9+", "9+"], [badge_label(1), badge_label(9), badge_label(10), badge_label(120)]);
    assert_eq!(badge_label(0), "0");
}

#[test]
fn badge_text_reads_on_every_theme() {
    // Le chiffre est dans la couleur du fond, sur la pastille d'accent : il se lit comme le texte d'un lien.
    for (name, _) in THEMES {
        let prefs = Prefs { theme: name.to_string(), ..Prefs::parse("") };
        let t = Theme::of(&prefs, gpui::WindowAppearance::Light);
        assert!(contrast(t.accent, t.bg) >= 4.5, "{name} : {} sur la pastille", contrast(t.accent, t.bg));
    }
}

#[test]
fn folded_bar_button_hides_what_it_covers() {
    // Le bouton de la barre repliée passe sur du texte : opaque, fermé ou ouvert, et son signe s'y lit, sur tous les thèmes.
    for (name, _) in THEMES {
        let prefs = Prefs { theme: name.to_string(), ..Prefs::parse("") };
        let t = Theme::of(&prefs, gpui::WindowAppearance::Light);
        for open in [false, true] {
            let fill = editor::bar_fill(&t, open);
            assert!(fill.a == 1., "{name} {open} : fond translucide {fill:?}");
            let sign = if open { t.accent } else { t.soft };
            assert!(contrast(sign, fill) >= 3., "{name} {open} : signe {} sur le fond", contrast(sign, fill));
        }
    }
}

/// Les raccourcis changés sont relus au lancement ; une ligne fautive du fichier est écartée
/// et signalée, les autres s'appliquent.
#[gpui::test]
fn keys_survive_a_restart(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-keys-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    vault::save_file("keys", "nav::ShowTags=secondary-shift-y\nno::Such=f8\n");
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| s.changes.clone()), keys::Changes::from([("nav::ShowTags".to_string(), "secondary-shift-y".to_string())]));
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Failed, "no::Such=f8")));
    cx.simulate_keystrokes("secondary-t");
    assert_ne!(shell.read_with(cx, |s, _| s.nav.mode), nav::Mode::Tags);
    cx.simulate_keystrokes("secondary-shift-y");
    assert_eq!(shell.read_with(cx, |s, _| s.nav.mode), nav::Mode::Tags);
    // Un fichier d'avant cette version, ou vide : les raccourcis d'origine.
    let _ = fs::remove_dir_all(&root);
}

/// Le second pane revient au lancement, avec sa largeur ; un fichier disparu, ou une config
/// d'avant les panes, laisse un seul pane.
#[gpui::test]
fn panes_come_back(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-panes-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    let (left, right) = (root.join("Gauche.md"), root.join("Droite.md"));
    fs::write(&left, "# Gauche\n").unwrap();
    fs::write(&right, "# Droite\n").unwrap();
    vault::save_file("panes", &format!("0.4\n{}\n", right.display()));
    cx.update(bind_keys);
    cx.update(init_fonts);
    // La note de droite est la dernière ouverte : le premier pane prend la suivante.
    let recent = vec![right.clone(), left.clone()];
    let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), recent, window, cx));
    cx.run_until_parked();
    let open = shell.read_with(cx, |s, _| (s.active, s.pane_ratio, s.panes.iter().map(|pane| pane.path.clone()).collect::<Vec<_>>()));
    assert_eq!(open, (0, 0.4, vec![Some(left.clone()), Some(right.clone())]));
    cx.simulate_input("x");
    assert_eq!(shell.read_with(cx, |s, cx| s.panes[0].editor.read(cx).text().to_string()), "# Gauche\nx");
    let _ = fs::remove_dir_all(&root);
}

/// Sans coffre, l'accueil en crée un d'un clic, dans `Notes` du dossier personnel.
#[gpui::test]
fn starts_in_a_new_vault(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-start-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(None, Vec::new(), window, cx));
    cx.run_until_parked();
    let at = cx.debug_bounds("welcome-create").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    let notes = root.join(".config").join("Notes");
    assert!(notes.is_dir());
    assert_eq!(shell.read_with(cx, |s, _| s.vault.clone()), Some(notes.clone()));
    // Sous Windows, le chemin se montre en entier.
    if !cfg!(windows) {
        assert_eq!(vault::shown(&notes), "~/Notes");
    }
    let _ = fs::remove_dir_all(&root);
}

/// Une note s'ouvre sur son corps : la première touche ne casse pas l'en-tête YAML et ne
/// renomme pas la note par son titre, ce qui réécrirait les liens des autres notes.
#[gpui::test]
fn opens_on_the_body(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-body-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    let plan = root.join("Plan.md");
    fs::write(&plan, "---\ntags: [a]\n---\n# Plan\n\nÉtapes\n").unwrap();
    fs::write(root.join("Accueil.md"), "# Accueil\n\n[[Plan]]\n").unwrap();
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (_shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), vec![plan.clone()], window, cx));
    cx.run_until_parked();
    cx.simulate_input("x");
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&plan).unwrap(), "---\ntags: [a]\n---\n# Plan\n\nxÉtapes\n");
    assert_eq!(fs::read_to_string(root.join("Accueil.md")).unwrap(), "# Accueil\n\n[[Plan]]\n");

    // Une nouvelle note écrite puis vidée ne laisse rien en partant.
    cx.simulate_keystrokes("secondary-n");
    cx.simulate_input("Brouillon");
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert!(root.join("Brouillon.md").is_file());
    cx.simulate_keystrokes("secondary-a backspace");
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    cx.simulate_keystrokes("secondary-n");
    cx.run_until_parked();
    let mut notes: Vec<_> = fs::read_dir(&root).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name()).filter(|n| n.to_string_lossy().ends_with(".md")).collect();
    notes.sort();
    assert_eq!(notes, ["Accueil.md", "Plan.md"]);
    let _ = fs::remove_dir_all(&root);
}

/// La capture (`bref --capture`) : une ligne tapée, Entrée, elle est dans la note du jour
/// et la fenêtre s'est refermée. Échap referme sans rien écrire.
#[gpui::test]
fn capture_window(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-capture-window-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    let note = root.join(format!("{}.md", date_name(today())));
    cx.update(bind_keys);
    cx.update(init_fonts);
    let closed = std::rc::Rc::new(std::cell::Cell::new(0));
    cx.update(|cx| {
        let closed = closed.clone();
        cx.on_window_closed(move |_| closed.set(closed.get() + 1)).detach();
    });
    {
        let (capture, cx) = cx.add_window_view(|window, cx| Capture::new(root.clone(), window, cx));
        cx.run_until_parked();
        // Un champ de saisie ne propose rien, même avant la première frappe.
        assert_eq!(capture.read_with(cx, |c, cx| c.field.read(cx).listed()), 0);
        cx.simulate_input("rappeler le plombier");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
    }
    assert_eq!(fs::read_to_string(&note).unwrap(), format!("# {}\n\n- rappeler le plombier\n", date_name(today())));
    assert_eq!(closed.get(), 1);
    {
        let (_, cx) = cx.add_window_view(|window, cx| Capture::new(root.clone(), window, cx));
        cx.run_until_parked();
        cx.simulate_input("à oublier");
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
    }
    assert!(!fs::read_to_string(&note).unwrap().contains("oublier"));
    assert_eq!(closed.get(), 2);
    // Une très longue ligne : rien n'est coupé ni refusé. Le texte défile sous le curseur,
    // qui reste dans le cadre, à la frappe comme au retour en début de ligne.
    let long = "mot ".repeat(250);
    {
        let (capture, cx) = cx.add_window_view(|window, cx| Capture::new(root.clone(), window, cx));
        cx.run_until_parked();
        let caret_inside = |cx: &mut gpui::VisualTestContext| capture.read_with(cx, |c, cx| c.field.read(cx).caret_inside());
        cx.simulate_input(&long);
        cx.run_until_parked();
        assert_eq!(caret_inside(cx), Some(true), "le curseur suit la frappe");
        cx.simulate_keystrokes("home");
        cx.run_until_parked();
        assert_eq!(caret_inside(cx), Some(true), "le curseur revient en vue au début");
        cx.simulate_keystrokes("end enter");
        cx.run_until_parked();
    }
    assert!(fs::read_to_string(&note).unwrap().ends_with(&format!("- {}\n", long.trim())));
    assert_eq!(closed.get(), 3);
    let _ = fs::remove_dir_all(&root);
}

/// La page vide : la date du jour, les touches utiles et les notes à relire, sous la ligne
/// où l'on écrit. Tout tient dans une fenêtre étroite, et s'efface à la première frappe.
#[gpui::test]
fn empty_page(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-empty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    let week = date_name(markdown::day_of(markdown::day_number(today()) - 7));
    let (daily, old) = (root.join(format!("{week}.md")), root.join("Ancienne.md"));
    fs::write(&daily, format!("# {week}\n")).unwrap();
    fs::write(&old, "# Ancienne\n").unwrap();
    let long_ago = std::time::SystemTime::now() - Duration::from_secs(100 * 86_400);
    fs::File::options().write(true).open(&old).unwrap().set_modified(long_ago).unwrap();
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
    cx.run_until_parked();
    let open = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.path.clone());
    let text_of = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).text().to_string());
    assert_eq!(open(cx), None);

    // Le logo est dans le rail, quel que soit le système.
    assert!(cx.debug_bounds("rail-logo").is_some());
    // Les notes à relire : la note du jour d'il y a une semaine, puis une note ancienne.
    assert_eq!(shell.read_with(cx, |s, _| s.recalled().iter().map(|r| r.name.clone()).collect::<Vec<_>>()), [week.clone(), "Ancienne".to_string()]);
    // Dans la fenêtre la plus étroite, rien ne dépasse du pane.
    cx.simulate_resize(size(px(360.), px(640.)));
    cx.run_until_parked();
    let pane = cx.debug_bounds("pane-0").unwrap();
    assert!(pane.size.width < px(360.), "{pane:?}");
    for part in ["empty-today", "empty-key-0", "empty-key-1", "empty-key-2", "empty-recall-0", "empty-recall-1"] {
        let bounds = cx.debug_bounds(part).unwrap_or_else(|| panic!("{part} absent"));
        assert!(bounds.left() >= pane.left() && bounds.right() <= pane.right(), "{part} dépasse : {bounds:?} hors de {pane:?}");
    }
    cx.simulate_resize(size(px(860.), px(720.)));
    cx.run_until_parked();

    // Un clic sur une note à relire l'ouvre.
    let at = cx.debug_bounds("empty-recall-1").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(open(cx), Some(old.clone()));
    // Une note qui a du texte ne montre pas la page vide : là où était la date, le clic
    // tombe dans la note. (gpui garde les bornes d'un élément disparu : on clique.)
    cx.run_until_parked();
    let at = cx.debug_bounds("empty-today").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(open(cx), Some(old.clone()));

    // Une touche de la page vide se clique : ici la palette.
    cx.simulate_keystrokes("secondary-n");
    cx.run_until_parked();
    let at = cx.debug_bounds("empty-key-0").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert!(shell.read_with(cx, |s, _| s.palette.is_some()));
    cx.simulate_keystrokes("escape");

    // La date ouvre la note du jour.
    let at = cx.debug_bounds("empty-today").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(open(cx), Some(root.join(format!("{}.md", date_name(today())))));

    // La première frappe efface tout : la page est à ce qu'on écrit.
    cx.simulate_keystrokes("secondary-n");
    cx.run_until_parked();
    cx.simulate_input("x");
    cx.run_until_parked();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(text_of(cx), "x");
    let _ = fs::remove_dir_all(&root);
}
