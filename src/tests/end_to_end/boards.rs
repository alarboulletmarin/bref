//! Kanban boards: columns, cards, fields, drag and drop.

use super::*;

/// `here`: a plain note to come back to between boards.
pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext, here: &Path) {
    let (shell, root) = (&j.shell, &j.root);
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Kanban : la palette crée un tableau, qui s'ouvre avec ses trois colonnes. Tout s'y
    // écrit sur place : « + » ouvre une carte, Entrée la valide et en rouvre une, Échap
    // arrête ; un double-clic réécrit une carte ou un titre. Déplacer ou cocher une carte
    // réécrit sa ligne. Rien ne fait quitter le tableau, sauf son bouton.
    // La palette est une ligne de saisie comme les autres : un mot s'y efface d'un coup.
    let word = if cfg!(target_os = "macos") { "alt" } else { "ctrl" };
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("tableau zzz");
    cx.simulate_keystrokes(&format!("{word}-backspace"));
    cx.simulate_input("kanban");
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    let on_board = |cx: &mut gpui::VisualTestContext| cx.update(|window, cx| shell.read(cx).board_focus.is_focused(window));
    assert!(shell.read_with(cx, |s, cx| s.board_shown(cx)) && on_board(cx));
    assert_eq!(text(cx), "---\nkanban: true\n---\n# Tableau\n\n## À faire\n\n## En cours\n\n## Fait\n");
    shell.update_in(cx, |s, window, cx| s.board_write(kanban::Slot::New(0), window, cx));
    cx.simulate_input("Écrire");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("Relire");
    cx.simulate_keystrokes("backspace enter escape");
    cx.run_until_parked();
    assert!(text(cx).contains("## À faire\n- [ ] Écrire\n- [ ] Relir\n\n## En cours") && shell.read_with(cx, |s, cx| s.board_field.is_none() && s.board_shown(cx)) && on_board(cx));
    shell.update_in(cx, |s, window, cx| s.board_write(kanban::Slot::Card(0, 1), window, cx));
    cx.simulate_input("e");
    cx.simulate_keystrokes("enter");
    shell.update_in(cx, |s, window, cx| s.board_write(kanban::Slot::Column(1), window, cx));
    cx.simulate_input(" de route");
    // Ouvrir un autre champ valide celui qu'on écrivait.
    shell.update_in(cx, |s, window, cx| s.board_write(kanban::Slot::NewColumn, window, cx));
    cx.simulate_input("Plus tard");
    cx.simulate_keystrokes("enter");
    assert!(text(cx).contains("- [ ] Relire\n\n## En cours de route\n") && text(cx).ends_with("## Fait\n\n## Plus tard\n"));
    // Une carte s'écrit dans une vraie ligne de saisie : un mot effacé d'un coup, le curseur
    // qui va au début, une sélection remplacée, tout copié puis collé à la fin.
    shell.update_in(cx, |s, window, cx| s.board_write(kanban::Slot::New(3), window, cx));
    cx.simulate_input("Un deux trois");
    cx.simulate_keystrokes(&format!("{word}-backspace home {word}-shift-right"));
    cx.simulate_input("Zéro");
    cx.simulate_keystrokes("secondary-a secondary-c end");
    cx.simulate_input("+");
    cx.simulate_keystrokes("secondary-v left delete enter escape");
    assert!(text(cx).ends_with("## Plus tard\n- [ ] Zéro deux +Zéro deux\n"), "{}", text(cx));
    shell.update(cx, |s, cx| s.board_edit(|text| kanban::retitle(text, (3, 0), ""), cx));
    // Une carte prise à la souris quitte sa place ; celle qu'elle prendrait suit le pointeur :
    // sous une colonne, son bas ; sur une carte, avant ou après elle selon la moitié visée.
    cx.run_until_parked();
    let held = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.board_drag);
    let (none, left) = (gpui::Modifiers::none(), gpui::MouseButton::Left);
    let (first, second) = (cx.debug_bounds("card-0-0").unwrap(), cx.debug_bounds("card-0-1").unwrap());
    let under = cx.debug_bounds("column-rest-2").unwrap().center();
    cx.simulate_mouse_down(first.center(), left, none);
    cx.simulate_mouse_move(first.center() + point(px(6.), px(6.)), left, none);
    cx.simulate_mouse_move(second.center() + point(px(0.), px(4.)), left, none);
    assert_eq!(held(cx), Some(((0, 0), (0, 2))));
    // La carte suivante est remontée à la place de celle qu'on tient.
    cx.simulate_mouse_move(first.center() - point(px(0.), px(4.)), left, none);
    assert_eq!(held(cx), Some(((0, 0), (0, 1))));
    cx.simulate_mouse_move(under, left, none);
    assert_eq!(held(cx), Some(((0, 0), (2, 0))));
    cx.simulate_mouse_up(under, left, none);
    cx.run_until_parked();
    assert_eq!(held(cx), None);
    // Le geste s'annule et se rétablit depuis le tableau, sans passer par le texte.
    cx.simulate_keystrokes("secondary-z");
    assert!(text(cx).contains("## À faire\n- [ ] Écrire\n- [ ] Relire\n") && on_board(cx));
    cx.simulate_keystrokes("secondary-shift-z");
    assert!(text(cx).contains("## Fait\n- [ ] Écrire\n"));
    shell.update(cx, |s, cx| s.board_edit(|text| kanban::tick(text, (2, 0)), cx));
    assert!(text(cx).contains("## À faire\n- [ ] Relire\n") && text(cx).contains("## Fait\n- [x] Écrire\n"));
    // Le bouton du coin, à gauche de la copie dans le texte comme dans le tableau.
    let corner = cx.update(|window, _| window.viewport_size());
    cx.simulate_click(point(corner.width - px(65.), corner.height - px(31.)), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, cx| !s.board_shown(cx)) && !on_board(cx));
    // Un geste du tableau, une étape d'annulation.
    cx.simulate_keystrokes("secondary-z secondary-z");
    assert!(text(cx).contains("## À faire\n- [ ] Écrire\n- [ ] Relire\n"));
    cx.simulate_click(point(corner.width - px(65.), corner.height - px(31.)), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, cx| s.board_shown(cx)));
    let board_file = shell.read_with(cx, |s, _| s.path.clone());
    shell.update(cx, |s, cx| s.open_note(here, cx));
    j.settle(cx);
    // Écrire la clé dans le texte d'une note ne la change pas en tableau sous la frappe :
    // on finit sa ligne, et c'est le bouton du coin qui montre le tableau.
    cx.simulate_keystrokes("secondary-n");
    cx.run_until_parked();
    cx.simulate_input("---\nkanban: true\n---\n# Semaine\n\n## Lundi\n");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, cx| !s.board_shown(cx) && kanban::is_board(s.editor.read(cx).text())));
    cx.simulate_click(point(corner.width - px(65.), corner.height - px(31.)), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, cx| s.board_shown(cx)));
    j.settle(cx);
    shell.update(cx, |s, cx| s.open_note(here, cx));
    j.settle(cx);
    for file in board_file.into_iter().chain([root.join("Tableau.md"), root.join("Semaine.md")]) {
        fs::remove_file(file).ok();
    }
    j.settle(cx);
}
