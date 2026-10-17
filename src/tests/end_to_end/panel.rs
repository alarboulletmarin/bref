//! The side panel: tree, new note in a folder, divider, recent notes, graph.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let (shell, root) = (&j.shell, &j.root);
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Arbre : Ctrl+E l'ouvre et lui donne le focus ; les flèches affichent un aperçu,
    // qui ne compte pas comme une ouverture.
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.simulate_keystrokes("secondary-e up");
    assert_eq!(shell.read_with(cx, |s, _| (s.nav.mode, s.nav.panel)), (Mode::Tree, Panel::Split));
    assert_eq!(text(cx), "# Courses\n\n- lait #maison\n[[Test]]");
    assert!(shell.read_with(cx, |s, _| s.preview && s.recent[0] == root.join("Idées.md")));
    // Le dossier se déplie avec Droite ; Entrée ouvre la note et rend la main à l'éditeur.
    cx.simulate_keystrokes("up right down");
    // Le bouton de repli replie tout l'arbre, puis le déplie tout entier.
    let open = |s: &mut Shell| {
        s.fold_all();
        s.nav.open.len()
    };
    assert_eq!(shell.update(cx, |s, _| (open(s), open(s))), (0, 1));
    assert_eq!(text(cx), "# Plan\n\n[[Courses]]\n");
    cx.simulate_keystrokes("enter");
    assert!(shell.read_with(cx, |s, _| !s.preview && s.recent[0] == root.join("Projets/Plan.md")));
    assert!(cx.update(|window, cx| shell.read(cx).editor.focus_handle(cx).is_focused(window)));

    // Nouvelle note depuis l'arbre : rangée dans le dossier de la sélection.
    cx.simulate_keystrokes("secondary-n");
    cx.simulate_input("# Sous");
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert!(root.join("Projets/Sous.md").is_file());

    // Séparateur : il règle la largeur, et replie le panneau en bout de course.
    let drag = |cx: &mut gpui::VisualTestContext, from: f32, to: f32| {
        let at = |x: f32| point(px(x), px(300.));
        cx.simulate_mouse_down(at(from), MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(at(to), MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(at(to), MouseButton::Left, gpui::Modifiers::none());
    };
    drag(cx, 302., 450.);
    assert_eq!(vault::load_layout(), "tree split 410 0\n");
    drag(cx, 452., 60.);
    assert_eq!(shell.read_with(cx, |s, _| s.nav.panel), Panel::Rail);

    // Récents, plein écran (Ctrl+M), puis Échap rend la place et le focus à la note.
    cx.simulate_keystrokes("secondary-r secondary-m");
    assert_eq!(shell.read_with(cx, |s, _| (s.nav.mode, s.nav.panel)), (Mode::Recent, Panel::Full));
    cx.simulate_keystrokes("down escape");
    assert_eq!(shell.read_with(cx, |s, _| s.nav.panel), Panel::Split);
    assert_eq!(text(cx), "# Sous");
    // Redemander le mode affiché : d'abord le focus, puis le repli sur le rail.
    cx.simulate_keystrokes("secondary-r secondary-r");
    assert_eq!(vault::load_layout(), "recent rail 410 0\n");

    // Graphe : une arête par wikilien résolu. Tab parcourt les notes liées à la
    // note ouverte, en aperçu ; Entrée ouvre celle qui est sélectionnée.
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("cou");
    cx.simulate_keystrokes("enter secondary-g");
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| (s.nav.mode, s.nav.panel)), (Mode::Graph, Panel::Split));
    assert_eq!(shell.read_with(cx, |s, cx| s.graph.read(cx).size()), (5, 2));
    // Un nœud tiré à la souris suit le pointeur.
    let spot = |cx: &mut gpui::VisualTestContext| {
        shell.read_with(cx, |s, cx| s.graph.read(cx).spot(&root.join("Courses.md")))
    };
    let from = spot(cx);
    let to = from + point(px(40.), px(30.));
    cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    assert!((spot(cx).x - to.x).abs() < px(1.) && (spot(cx).y - to.y).abs() < px(1.));
    let mut linked = Vec::new();
    for _ in 0..2 {
        cx.simulate_keystrokes("tab");
        linked.push(shell.read_with(cx, |s, _| vault::stem(s.path.as_ref().unwrap())));
        assert!(shell.read_with(cx, |s, _| s.preview && s.recent[0] == root.join("Courses.md")));
    }
    linked.sort();
    assert_eq!(linked, ["Plan", "Test"]);
    cx.simulate_keystrokes("enter");
    assert!(shell.read_with(cx, |s, _| !s.preview && s.recent[0] == *s.path.as_ref().unwrap()));
    assert!(cx.update(|window, cx| shell.read(cx).editor.focus_handle(cx).is_focused(window)));
    // Plein écran : la sélection ne déplace plus la note ; Entrée lui rend sa place.
    cx.simulate_keystrokes("secondary-g secondary-m tab");
    let open = shell.read_with(cx, |s, _| s.path.clone());
    assert!(shell.read_with(cx, |s, _| s.nav.panel == Panel::Full && s.nav.sel != s.path));
    cx.simulate_keystrokes("enter");
    assert!(shell.read_with(cx, |s, _| s.nav.panel == Panel::Split && s.path != open));
}
