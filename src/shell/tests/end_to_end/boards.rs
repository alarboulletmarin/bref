//! Kanban boards: columns, cards, fields, drag and drop.

use super::*;

/// `here`: a plain note to come back to between boards.
pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext, here: &Path) {
    let (shell, root) = (&j.shell, &j.root);
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
        // Kanban: the palette creates a board, which opens with its three columns. Everything is
        // written in place: "+" opens a card, Enter confirms it and opens another, Escape
        // stops; a double-click rewrites a card or a heading. Moving or checking a card
        // rewrites its line. Nothing leaves the board, except its button.
        // The palette is an input line like the others: a word is deleted at once.
    let word = if cfg!(target_os = "macos") { "alt" } else { "ctrl" };
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("tableau zzz");
    cx.simulate_keystrokes(&format!("{word}-backspace"));
    cx.simulate_input("kanban");
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    let on_board = |cx: &mut gpui::VisualTestContext| cx.update(|window, cx| shell.read(cx).pane().board.focus.is_focused(window));
    assert!(shell.read_with(cx, |s, cx| s.board_shown(cx)) && on_board(cx));
    assert_eq!(text(cx), "---\nkanban: true\n---\n# Tableau\n\n## À faire\n\n## En cours\n\n## Fait\n");
    shell.update_in(cx, |s, window, cx| s.board_write(kanban::Slot::New(0), window, cx));
    cx.simulate_input("Écrire");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("Relire");
    cx.simulate_keystrokes("backspace enter escape");
    cx.run_until_parked();
    assert!(text(cx).contains("## À faire\n- [ ] Écrire\n- [ ] Relir\n\n## En cours") && shell.read_with(cx, |s, cx| s.pane().board.field.is_none() && s.board_shown(cx)) && on_board(cx));
    shell.update_in(cx, |s, window, cx| s.board_write(kanban::Slot::Card(0, 1), window, cx));
    cx.simulate_input("e");
    cx.simulate_keystrokes("enter");
    shell.update_in(cx, |s, window, cx| s.board_write(kanban::Slot::Column(1), window, cx));
    cx.simulate_input(" de route");
        // Opening another field confirms the one being written.
    shell.update_in(cx, |s, window, cx| s.board_write(kanban::Slot::NewColumn, window, cx));
    cx.simulate_input("Plus tard");
    cx.simulate_keystrokes("enter");
    assert!(text(cx).contains("- [ ] Relire\n\n## En cours de route\n") && text(cx).ends_with("## Fait\n\n## Plus tard\n"));
        // A card is written in a real input line: a word deleted at once, the cursor
        // going to the start, a selection replaced, everything copied then pasted at the end.
    shell.update_in(cx, |s, window, cx| s.board_write(kanban::Slot::New(3), window, cx));
    cx.simulate_input("Un deux trois");
    cx.simulate_keystrokes(&format!("{word}-backspace home {word}-shift-right"));
    cx.simulate_input("Zéro");
    cx.simulate_keystrokes("secondary-a secondary-c end");
    cx.simulate_input("+");
    cx.simulate_keystrokes("secondary-v left delete enter escape");
    assert!(text(cx).ends_with("## Plus tard\n- [ ] Zéro deux +Zéro deux\n"), "{}", text(cx));
    shell.update(cx, |s, cx| s.board_edit(|text| kanban::retitle(text, (3, 0), ""), cx));
        // A card picked up with the mouse leaves its place; the place it would take follows the pointer:
        // below a column, its bottom; over a card, before or after it depending on the half aimed at.
    cx.run_until_parked();
    let held = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.pane().board.drag);
    let (none, left) = (gpui::Modifiers::none(), gpui::MouseButton::Left);
    let (first, second) = (cx.debug_bounds("card-0-0").unwrap(), cx.debug_bounds("card-0-1").unwrap());
    let under = cx.debug_bounds("column-rest-2").unwrap().center();
        // Columns are as tall as the board, whatever their cards, and the room under the cards is inside the column.
    let (tall, short) = (cx.debug_bounds("column-0").unwrap(), cx.debug_bounds("column-2").unwrap());
    assert_eq!((tall.top(), tall.bottom()), (short.top(), short.bottom()));
    assert!(short.contains(&under) && short.size.height > px(300.), "{short:?}");
    cx.simulate_mouse_down(first.center(), left, none);
    cx.simulate_mouse_move(first.center() + point(px(6.), px(6.)), left, none);
    cx.simulate_mouse_move(second.center() + point(px(0.), px(4.)), left, none);
    assert_eq!(held(cx), Some(((0, 0), (0, 2))));
        // The next card has moved up to the place of the one being held.
    cx.simulate_mouse_move(first.center() - point(px(0.), px(4.)), left, none);
    assert_eq!(held(cx), Some(((0, 0), (0, 1))));
    cx.simulate_mouse_move(under, left, none);
    assert_eq!(held(cx), Some(((0, 0), (2, 0))));
    cx.simulate_mouse_up(under, left, none);
    cx.run_until_parked();
    assert_eq!(held(cx), None);
        // The gesture is undone and redone from the board, without going through the text.
    cx.simulate_keystrokes("secondary-z");
    assert!(text(cx).contains("## À faire\n- [ ] Écrire\n- [ ] Relire\n") && on_board(cx));
    cx.simulate_keystrokes("secondary-shift-z");
    assert!(text(cx).contains("## Fait\n- [ ] Écrire\n"));
    shell.update(cx, |s, cx| s.board_edit(|text| kanban::tick(text, (2, 0)), cx));
    assert!(text(cx).contains("## À faire\n- [ ] Relire\n") && text(cx).contains("## Fait\n- [x] Écrire\n"));
        // The corner button, left of the copy button in the text as in the board.
    let corner = cx.update(|window, _| window.viewport_size());
    cx.simulate_click(point(corner.width - px(65.), corner.height - px(31.)), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, cx| !s.board_shown(cx)) && !on_board(cx));
        // One board gesture, one undo step.
    cx.simulate_keystrokes("secondary-z secondary-z");
    assert!(text(cx).contains("## À faire\n- [ ] Écrire\n- [ ] Relire\n"));
    cx.simulate_click(point(corner.width - px(65.), corner.height - px(31.)), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, cx| s.board_shown(cx)));
    let board_file = shell.read_with(cx, |s, _| s.pane().path.clone());
    shell.update(cx, |s, cx| s.open_note(here, cx));
    j.settle(cx);
        // Writing the key in a note's text does not turn it into a board while typing:
        // you finish your line, and it is the corner button that shows the board.
    cx.simulate_keystrokes("secondary-n");
    cx.run_until_parked();
    cx.simulate_input("---\nkanban: true\n---\n# Semaine\n\n## Lundi\n");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, cx| !s.board_shown(cx) && kanban::is_board(s.pane().editor.read(cx).text())));
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
