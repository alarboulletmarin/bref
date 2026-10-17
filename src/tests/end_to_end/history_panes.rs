//! Back and forward, shortcuts shown in menus, two panes.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let (shell, root) = (&j.shell, &j.root);
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Précédent / suivant : l'historique de ce qui a été affiché, comme dans un navigateur.
    let (hist_a, hist_b, hist_csv) = (root.join("Hist A.md"), root.join("Hist B.md"), root.join("hist.csv"));
    let long: String = (0..200).map(|i| format!("ligne {i}\n")).collect();
    fs::write(&hist_a, format!("# Hist A\n\n{long}\n[[Hist B]]\n")).unwrap();
    fs::write(&hist_b, "# Hist B\n").unwrap();
    fs::write(&hist_csv, "a,b\n1,2\n3,4\n5,6\n").unwrap();
    j.settle(cx);
    let (back, forward) = if cfg!(target_os = "macos") { ("cmd-[", "cmd-]") } else { ("alt-left", "alt-right") };
    let seen = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| (s.pane().file_path().cloned(), s.pane().path.clone()));
    let on_note = |path: &Path| (None, Some(path.to_path_buf()));
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&hist_a, window, cx));
    // Loin dans la note : le curseur et le défilement seront retrouvés au retour.
    cx.simulate_keystrokes("secondary-end up up up");
    let place = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).view());
    assert!(place.0 > 1000 && place.1 > 0., "{place:?}");
    // Suivre un lien, revenir, repartir.
    shell.update_in(cx, |s, window, cx| s.follow(&Link::Wiki("Hist B".into()), window, cx));
    assert_eq!(seen(cx), on_note(&hist_b));
    assert!(shell.read_with(cx, |s, _| s.history.can_back() && !s.history.can_forward()));
    cx.simulate_keystrokes(back);
    assert_eq!(seen(cx), on_note(&hist_a));
    assert_eq!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).view()), place);
    cx.simulate_input("x");
    assert!(text(cx).contains("x"), "la saisie est dans la note retrouvée");
    cx.simulate_keystrokes("backspace");
    assert!(shell.read_with(cx, |s, _| s.history.can_forward()));
    cx.simulate_keystrokes(forward);
    assert_eq!(seen(cx), on_note(&hist_b));
    // Le bouton du rail.
    let at = cx.debug_bounds("nav-back").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(seen(cx), on_note(&hist_a));
    // Ouvrir autre chose après un retour : « suivant » n'a plus où aller. Un tableau compte.
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&hist_csv, window, cx));
    assert_eq!(seen(cx).0, Some(hist_csv.clone()));
    assert!(!shell.read_with(cx, |s, _| s.history.can_forward()));
    cx.simulate_keystrokes("down right");
    cx.simulate_keystrokes(forward);
    assert_eq!(seen(cx).0, Some(hist_csv.clone()));
    cx.simulate_keystrokes(back);
    assert_eq!(seen(cx), on_note(&hist_a));
    cx.simulate_keystrokes(forward);
    assert_eq!(seen(cx).0, Some(hist_csv.clone()));
    assert_eq!(shell.read_with(cx, |s, cx| s.shown_sheet().unwrap().read(cx).whereabouts().0), (1, 1), "la cellule est retrouvée");
    // Les boutons précédent et suivant de la souris.
    let middle = gpui::point(px(500.), px(400.));
    let navigate = |cx: &mut gpui::VisualTestContext, to| {
        cx.simulate_mouse_down(middle, MouseButton::Navigate(to), gpui::Modifiers::none());
        cx.simulate_mouse_up(middle, MouseButton::Navigate(to), gpui::Modifiers::none());
    };
    navigate(cx, gpui::NavigationDirection::Back);
    assert_eq!(seen(cx), on_note(&hist_a));
    // Un aperçu n'entre pas dans l'historique : « précédent » ramène d'où l'on vient, et
    // « suivant » mène toujours au tableau.
    shell.update_in(cx, |s, window, cx| {
        window.focus(&s.nav.focus);
        s.preview_note(&hist_b, cx);
    });
    assert_eq!(seen(cx), on_note(&hist_b));
    cx.simulate_keystrokes(back);
    assert_eq!(seen(cx), on_note(&hist_a));
    navigate(cx, gpui::NavigationDirection::Forward);
    assert_eq!(seen(cx).0, Some(hist_csv.clone()));
    // Une note déplacée est suivie.
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&hist_b, window, cx));
    let shelf = root.join("Étagère");
    fs::create_dir_all(&shelf).unwrap();
    shell.update(cx, |s, cx| s.move_into(&hist_b, &shelf, cx));
    let moved = shelf.join("Hist B.md");
    cx.simulate_keystrokes(back);
    assert_eq!(seen(cx).0, Some(hist_csv.clone()));
    cx.simulate_keystrokes(forward);
    assert_eq!(seen(cx), on_note(&moved));
    // Un fichier mis à la corbeille quitte l'historique ; un fichier supprimé par un autre
    // programme est sauté.
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Trash, Some(hist_csv.clone()), window, cx));
    cx.simulate_keystrokes(back);
    assert_eq!(seen(cx), on_note(&hist_a));
    fs::remove_file(&moved).unwrap();
    cx.simulate_keystrokes(forward);
    assert_eq!(seen(cx), on_note(&hist_a));
    assert!(!shell.read_with(cx, |s, _| s.history.can_forward()));
    // Depuis une note neuve, pas encore enregistrée : « précédent » ramène à la dernière.
    shell.update_in(cx, |s, window, cx| s.new_note_in(None, window, cx));
    assert_eq!(seen(cx), (None, None));
    cx.simulate_keystrokes(back);
    assert_eq!(seen(cx), on_note(&hist_a));

    // Les menus montrent, à côté de chaque action, le raccourci qui fait la même chose :
    // celui en vigueur, et rien pour une action qui n'en a pas.
    let keys = |cx: &mut gpui::VisualTestContext, what: nav::Do| cx.update(|_, cx| what.keys(cx));
    assert_eq!(keys(cx, nav::Do::Rename), "F2");
    assert_eq!(keys(cx, nav::Do::Duplicate), format!("{MOD}+D"));
    assert_eq!(keys(cx, nav::Do::Bold), format!("{MOD}+B"));
    assert_eq!(keys(cx, nav::Do::OpenLink), format!("{MOD}+clic"));
    assert_eq!(keys(cx, nav::Do::Icon), "");
    shell.update(cx, |s, cx| s.key_set("nav::Rename", "f6".into(), &[], cx));
    assert_eq!(keys(cx, nav::Do::Rename), "F6");
    shell.update(cx, |s, cx| s.key_set("nav::Rename", String::new(), &[], cx));
    assert_eq!(keys(cx, nav::Do::Rename), "");
    shell.update(cx, |s, cx| s.key_reset(None, cx));
    assert_eq!(keys(cx, nav::Do::Rename), "F2");

    // Deux panes côte à côte.
    let pane = |name: &str| root.join(format!("Pane {name}.md"));
    for name in ["A", "B", "D"] {
        fs::write(pane(name), format!("# Pane {name}\n\n")).unwrap();
    }
    fs::write(pane("C"), "# Pane C\n\n[[Pane A]]\n").unwrap();
    j.settle(cx);
    let open = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| (s.active, s.panes.iter().map(|pane| pane.path.clone()).collect::<Vec<_>>()));
    let both = |active: usize, left: &str, right: &str| (active, vec![Some(pane(left)), Some(pane(right))]);
    let pane_text = |cx: &mut gpui::VisualTestContext, pane: usize| shell.read_with(cx, |s, cx| s.panes[pane].editor.read(cx).text().to_string());
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&pane("A"), window, cx));
    assert_eq!(open(cx), (0, vec![Some(pane("A"))]));
    // Au clavier : le second pane s'ouvre sur la palette, pour choisir ce qui y va.
    cx.simulate_keystrokes("secondary-\\");
    assert_eq!(open(cx), (1, vec![Some(pane("A")), None]));
    assert!(shell.read_with(cx, |s, _| s.palette.is_some()));
    cx.simulate_input("pane b");
    cx.simulate_keystrokes("enter");
    assert_eq!(open(cx), both(1, "A", "B"));
    // Chaque pane s'écrit et s'enregistre ; Ctrl+1 et Ctrl+2 passent de l'un à l'autre.
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input("droite");
    cx.simulate_keystrokes("secondary-1 secondary-end");
    assert_eq!(open(cx).0, 0);
    cx.simulate_input("gauche");
    assert_eq!((pane_text(cx, 0), pane_text(cx, 1)), ("# Pane A\n\ngauche".to_string(), "# Pane B\n\ndroite".to_string()));
    j.settle(cx);
    assert_eq!(fs::read_to_string(pane("A")).unwrap(), "# Pane A\n\ngauche");
    assert_eq!(fs::read_to_string(pane("B")).unwrap(), "# Pane B\n\ndroite");
    // Une note déjà affichée ne s'ouvre pas deux fois : la saisie passe dans son pane.
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&pane("B"), window, cx));
    assert_eq!(open(cx), both(1, "A", "B"));
    cx.simulate_input("!");
    assert_eq!(pane_text(cx, 1), "# Pane B\n\ndroite!");
    // Ctrl+Entrée dans la palette : la note choisie s'ouvre dans l'autre pane.
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("pane c");
    cx.simulate_keystrokes("secondary-enter");
    assert_eq!(open(cx), both(0, "C", "B"));
    // Ctrl+Maj+clic sur un lien : la note liée s'ouvre à côté.
    let link = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).point_of("# Pane C\n\n[[Pa".len()).unwrap());
    cx.simulate_click(link, gpui::Modifiers { shift: true, ..gpui::Modifiers::secondary_key() });
    assert_eq!(open(cx), both(1, "C", "A"));
    // « Ouvrir à côté » dans le menu de l'arbre.
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::OpenAside, Some(pane("D")), window, cx));
    assert_eq!(open(cx), both(0, "D", "A"));
    // Renommer ou déplacer la note de l'autre pane : il suit.
    shell.update(cx, |s, cx| s.move_into(&pane("A"), &shelf, cx));
    assert_eq!(open(cx), (0, vec![Some(pane("D")), Some(shelf.join("Pane A.md"))]));
    // Glisser une note de l'arbre sur un pane l'y ouvre ; la moitié visée est teintée.
    shell.update(cx, |s, cx| {
        (s.nav.mode, s.nav.panel) = (nav::Mode::Tree, nav::Panel::Split);
        cx.notify();
    });
    cx.run_until_parked();
    let row = cx.debug_bounds("nav-row-Pane B").unwrap().center();
    let right = cx.debug_bounds("pane-1").unwrap().center();
    cx.simulate_mouse_down(row, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(row + gpui::point(px(12.), px(4.)), Some(MouseButton::Left), gpui::Modifiers::none());
    cx.simulate_mouse_move(right, Some(MouseButton::Left), gpui::Modifiers::none());
    assert_eq!(shell.read_with(cx, |s, _| s.pane_aim), Some((1, false)));
    cx.simulate_mouse_up(right, MouseButton::Left, gpui::Modifiers::none());
    assert_eq!(open(cx), both(1, "D", "B"));
    assert_eq!(shell.read_with(cx, |s, _| s.pane_aim), None);
    // Ctrl+W ferme le pane qui a la saisie ; ce qu'on y avait écrit est enregistré.
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input("?");
    cx.simulate_keystrokes("secondary-w");
    assert_eq!(open(cx), (0, vec![Some(pane("D"))]));
    assert_eq!(fs::read_to_string(pane("B")).unwrap(), "# Pane B\n\ndroite!?");
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input("seul");
    assert_eq!(text(cx), "# Pane D\n\nseul");
    cx.simulate_keystrokes("secondary-w");
    assert_eq!(open(cx).1.len(), 1, "le dernier pane ne se ferme pas");
    // Avec un seul pane, lâcher une note sur la moitié droite ouvre le second.
    let whole = cx.debug_bounds("pane-0").unwrap();
    let half = gpui::point(whole.right() - px(40.), whole.center().y);
    cx.simulate_mouse_down(row, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(row + gpui::point(px(12.), px(4.)), Some(MouseButton::Left), gpui::Modifiers::none());
    cx.simulate_mouse_move(half, Some(MouseButton::Left), gpui::Modifiers::none());
    assert_eq!(shell.read_with(cx, |s, _| s.pane_aim), Some((0, true)));
    cx.simulate_mouse_up(half, MouseButton::Left, gpui::Modifiers::none());
    assert_eq!(open(cx), both(1, "D", "B"));
    // La disposition est retenue : la largeur, et ce que montre le second pane.
    assert_eq!(vault::load_file("panes"), format!("0.5\n{}\n", pane("B").display()));
    cx.simulate_keystrokes("secondary-w");
    assert_eq!(vault::load_file("panes"), "0.5\n\n");
}
