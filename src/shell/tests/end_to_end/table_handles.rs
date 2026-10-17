//! Table rows and columns from the menu, the palette, the keyboard and the handles.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // A table: its rows and columns are handled from the menu, the palette or the keyboard,
    // and the cursor follows the gesture.
    let flat = |cx: &mut gpui::VisualTestContext| {
        markdown::table(&text(cx).lines().filter(|l| l.starts_with('|')).collect::<Vec<_>>().join("\n")).0.iter().map(|r| r.iter().map(|c| if c.starts_with("--") { "---" } else { c }).collect::<Vec<_>>().join("|")).collect::<Vec<_>>().join("/")
    };
    let op = |cx: &mut gpui::VisualTestContext, op: editor::TableOp| {
        shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Table(op), None, window, cx));
    };
    use editor::TableOp::*;
    let grid = "| Nom | Âge |\n| --- | --- |\n| Léa | 31  |\n| Max | 7   |\n\nsuite";
    j.load(cx, grid, grid.find("Léa").unwrap());
    op(cx, RowBelow);
    assert_eq!(flat(cx), "Nom|Âge/---|---/Léa|31/|/Max|7");
    cx.simulate_input("Zoé");
    assert_eq!(flat(cx), "Nom|Âge/---|---/Léa|31/Zoé|/Max|7");
    cx.simulate_keystrokes("alt-up");
    assert_eq!(flat(cx), "Nom|Âge/---|---/Zoé|/Léa|31/Max|7");
    cx.simulate_keystrokes("alt-up");
    assert_eq!(flat(cx), "Nom|Âge/---|---/Zoé|/Léa|31/Max|7", "la première ligne ne monte pas plus haut");
    op(cx, ColRight);
    cx.simulate_input("Ville");
    assert_eq!(flat(cx), "Nom||Âge/---|---|---/Zoé|Ville|/Léa||31/Max||7");
    op(cx, MoveColLeft);
    assert_eq!(flat(cx), "|Nom|Âge/---|---|---/Ville|Zoé|/|Léa|31/|Max|7");
    op(cx, DeleteCol);
    assert_eq!(flat(cx), "Nom|Âge/---|---/Zoé|/Léa|31/Max|7");
    op(cx, SortDesc);
    assert_eq!(flat(cx), "Nom|Âge/---|---/Zoé|/Max|7/Léa|31");
    // Two chosen rows go together; neither the header nor the body leaves a table without rows.
    j.load(cx, grid, grid.find("Léa").unwrap());
    cx.simulate_keystrokes("shift-down");
    cx.simulate_keystrokes("secondary-shift-backspace");
    assert_eq!(flat(cx), "Nom|Âge/---|---");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(flat(cx), "Nom|Âge/---|---/Léa|31/Max|7");
    // The whole table goes with its line break; undo brings it back.
    op(cx, DeleteTable);
    assert_eq!(text(cx), "\nsuite");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(flat(cx), "Nom|Âge/---|---/Léa|31/Max|7");
    // An indented table keeps its indent; a table alone in the note, without a final line break, disappears entirely.
    let indented = "  | a | b |\n  | - | - |\n  | 1 | 2 |";
    j.load(cx, indented, indented.find('1').unwrap());
    op(cx, RowBelow);
    assert!(text(cx).lines().all(|l| l.starts_with("  | ")) && text(cx).lines().count() == 4, "{:?}", text(cx));
    cx.simulate_keystrokes("secondary-z");
    j.load(cx, "| a |\n| - |\n| b |", 8);
    op(cx, DeleteTable);
    assert_eq!(text(cx), "");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), "| a |\n| - |\n| b |");
    // The handles: a grabbed column is copied, cleared, deleted; a row is dragged higher;
    // the "+" of a line inserts; the corner grabs the whole table.
    j.load(cx, grid, grid.find("Léa").unwrap());
    cx.run_until_parked();
    let center = |cx: &mut gpui::VisualTestContext, part: editor::Part| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).part_center(part).unwrap());
    let none = gpui::Modifiers::none();
    let at = center(cx, editor::Part::Col(1));
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-c");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("Âge\n31\n7"));
    cx.simulate_keystrokes("delete");
    assert_eq!(flat(cx), "Nom|/---|---/Léa|/Max|");
    cx.simulate_keystrokes("secondary-z");
    let at = center(cx, editor::Part::Col(0));
    cx.simulate_click(at, none);
    op(cx, DeleteCol);
    assert_eq!(flat(cx), "Âge/---/31/7");
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    let (from, to) = (center(cx, editor::Part::Row(2)), center(cx, editor::Part::JoinRow(1)));
    cx.simulate_mouse_down(from, MouseButton::Left, none);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    cx.simulate_mouse_up(to, MouseButton::Left, none);
    assert_eq!(flat(cx), "Nom|Âge/---|---/Max|7/Léa|31");
    // The moved table stays grabbed: « monter » grabs it again.
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    let at = center(cx, editor::Part::JoinCol(1));
    cx.simulate_click(at, none);
    assert_eq!(flat(cx), "Nom||Âge/---|---|---/Léa||31/Max||7");
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    let at = center(cx, editor::Part::Corner);
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-c");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("Nom\tÂge\nLéa\t31\nMax\t7"));
    // The limits of the handles. A table of one column and one row: deleting its column deletes it.
    use editor::Part;
    let one = "avant\n\n| a |\n| - |\n| b |\n\nsuite";
    j.load(cx, one, one.find('b').unwrap());
    cx.run_until_parked();
    let at = center(cx, Part::Col(0));
    cx.simulate_click(at, none);
    op(cx, DeleteCol);
    assert!(!text(cx).contains('|') && text(cx).contains("suite") && text(cx).starts_with("avant"), "{:?}", text(cx));
    // The header can be grabbed but not dragged; a row dropped where it is moves nothing, neither under the
    // last one, nor on its own line; a column dragged beyond the last one goes last.
    j.load(cx, grid, grid.find("Léa").unwrap());
    cx.run_until_parked();
    let before = flat(cx);
    let (head, below) = (center(cx, Part::Row(0)), center(cx, Part::JoinRow(2)));
    cx.simulate_mouse_down(head, MouseButton::Left, none);
    cx.simulate_mouse_move(below, MouseButton::Left, none);
    cx.simulate_mouse_up(below, MouseButton::Left, none);
    assert_eq!(flat(cx), before);
    for (row, to) in [(2, 3), (2, 2), (1, 1), (1, 2)] {
        let (from, to) = (center(cx, Part::Row(row)), center(cx, Part::JoinRow(to)));
        cx.simulate_mouse_down(from, MouseButton::Left, none);
        cx.simulate_mouse_move(to, MouseButton::Left, none);
        cx.simulate_mouse_up(to, MouseButton::Left, none);
        assert_eq!(flat(cx), before, "ligne {row} lâchée sur le trait {to}");
    }
    let (from, to) = (center(cx, Part::Col(0)), center(cx, Part::JoinCol(2)));
    cx.simulate_mouse_down(from, MouseButton::Left, none);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    cx.simulate_mouse_up(to, MouseButton::Left, none);
    assert_eq!(flat(cx), "Âge|Nom/---|---/31|Léa/7|Max");
    // Escape mid-drag: nothing moves.
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    let (from, to) = (center(cx, Part::Row(1)), center(cx, Part::JoinRow(3)));
    cx.simulate_mouse_down(from, MouseButton::Left, none);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(to, MouseButton::Left, none);
    assert_eq!(flat(cx), before);
    // The "+" of the last line adds a row at the end; the one of the first, a column at the start.
    let at = center(cx, Part::JoinRow(3));
    cx.simulate_click(at, none);
    assert_eq!(flat(cx), "Nom|Âge/---|---/Léa|31/Max|7/|");
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    let at = center(cx, Part::JoinCol(0));
    cx.simulate_click(at, none);
    assert_eq!(flat(cx), "|Nom|Âge/---|---|---/|Léa|31/|Max|7");
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    // The corner then Clear empties all cells without changing the shape; Escape releases the grab.
    let at = center(cx, Part::Corner);
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("delete");
    assert_eq!(flat(cx), "|/---|---/|/|");
    cx.simulate_keystrokes("secondary-z");
    let at = center(cx, Part::Corner);
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("secondary-shift-backspace");
    // Without a grab, the gesture applies to the cursor's row: the header, and the next one takes its place.
    assert_eq!(flat(cx), "Léa|31/---|---/Max|7");
    cx.simulate_keystrokes("secondary-z");
    // A column grabbed by its handle: the same gesture deletes it, it and not the row.
    let at = center(cx, Part::Col(0));
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-shift-backspace");
    assert_eq!(flat(cx), "Âge/---/31/7");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(flat(cx), "Nom|Âge/---|---/Léa|31/Max|7");
    // Thirty columns: those that go out of the frame have no handle on screen; a table
    // of more than 1,500 rows has none (it keeps Tab).
    let wide = format!("|{}\n|{}\n|{}\n", " colonne |".repeat(30), " --- |".repeat(30), " texte |".repeat(30));
    j.load(cx, &wide, 0);
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).part_center(Part::Col(0)).is_some() && s.pane().editor.read(cx).part_center(Part::Col(29)).is_none()));
    let big = format!("| a | b |\n| - | - |\n{}", "| 1 | 2 |\n".repeat(grid::BIG + 5));
    j.load(cx, &big, 0);
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).part_center(Part::Col(0)).is_none() && !s.pane().editor.read(cx).in_table()));
    // In the note's menu: the table gestures, and its palette.
    j.load(cx, grid, grid.find("Léa").unwrap());
    assert!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).in_table()));
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("colonne à droite");
    cx.simulate_keystrokes("down enter");
    assert_eq!(flat(cx), "Nom||Âge/---|---|---/Léa||31/Max||7");
    // The whole table grabbed (the corner): the column comes right of the last one.
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    let at = center(cx, Part::Corner);
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("colonne à droite");
    cx.simulate_keystrokes("down enter");
    assert_eq!(flat(cx), "Nom|Âge|/---|---|---/Léa|31|/Max|7|");
    // The note open here does not exist on disk: saving fails, and its notice clears.
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
}
