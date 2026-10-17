//! CSV and TSV files in the grid: columns, sorting, encodings, delimiters, copy.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let (shell, root) = (&j.shell, &j.root);
    // Columns and sorting of a CSV: insert, move, delete, duplicate, sort, and undo everything, the file
    // getting its bytes back. Empty cells go to the end in both directions; the refused column
    // is the last one; the header stays on top.
    let cols = root.join("cols.csv");
    let original = "id,name,score\n3,Zoé,10\n1,Max,\n2,Léa,9\n";
    fs::write(&cols, original).unwrap();
    shell.update(cx, |s, cx| {
        let (notes, dirs, images) = vault::scan(root);
        s.sync(notes, dirs, images, cx);
        s.open_note(&cols, cx);
    });
    let sheet = shell.read_with(cx, |s, _| s.shown_sheet().expect("the table is shown"));
    shell.update_in(cx, |s, window, cx| window.focus(&s.shown_sheet().unwrap().focus_handle(cx)));
    let all = |cx: &mut gpui::VisualTestContext| {
        sheet.read_with(cx, |sheet, _| (0..sheet.table().rows()).map(|r| sheet.table().row(r).join("|")).collect::<Vec<_>>().join("/"))
    };
    let on_disk = |cx: &mut gpui::VisualTestContext| {
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        fs::read_to_string(&cols).unwrap()
    };
    let table_op = |cx: &mut gpui::VisualTestContext, op| sheet.update(cx, |sheet, cx| sheet.table_op(op, cx));
    assert_eq!(all(cx), "id|name|score/3|Zoé|10/1|Max|/2|Léa|9");
    // Sort from the palette, on the cursor's column: "score" high to low, empty last.
    cx.simulate_keystrokes("down right right");
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("de A à Z");
    cx.simulate_keystrokes("down enter");
    assert_eq!(all(cx), "id|name|score/2|Léa|9/3|Zoé|10/1|Max|");
    table_op(cx, editor::TableOp::SortDesc);
    assert_eq!(all(cx), "id|name|score/3|Zoé|10/2|Léa|9/1|Max|");
    // Two equal rows keep their order; sorting a sorted table changes nothing (no undo step).
    let undo_len = sheet.read_with(cx, |sheet, _| sheet.undo_len());
    table_op(cx, editor::TableOp::SortDesc);
    assert_eq!(sheet.read_with(cx, |sheet, _| sheet.undo_len()), undo_len);
    assert_eq!(on_disk(cx), "id,name,score\n3,Zoé,10\n2,Léa,9\n1,Max,\n");
    cx.simulate_keystrokes("secondary-z secondary-z");
    assert_eq!(all(cx), "id|name|score/3|Zoé|10/1|Max|/2|Léa|9");
    assert_eq!(on_disk(cx), original, "undone, the file gets its bytes back");
    // An empty column on the right, a copy, a move, then everything undone.
    // Far right: the column exists, without writing anything to the file while it is empty.
    let width = |cx: &mut gpui::VisualTestContext| sheet.read_with(cx, |sheet, _| sheet.table().cols());
    table_op(cx, editor::TableOp::ColRight);
    assert_eq!((width(cx), all(cx)), (4, "id|name|score/3|Zoé|10/1|Max|/2|Léa|9".to_string()));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(width(cx), 3);
    // In the middle: each row that has cells to the right gets its own, empty.
    cx.simulate_keystrokes("secondary-home right");
    table_op(cx, editor::TableOp::ColLeft);
    assert_eq!(all(cx), "id||name|score/3||Zoé|10/1||Max|/2||Léa|9");
    assert_eq!(on_disk(cx), "id,,name,score\n3,,Zoé,10\n1,,Max,\n2,,Léa,9\n");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(all(cx), "id|name|score/3|Zoé|10/1|Max|/2|Léa|9");
    assert_eq!(on_disk(cx), original);
    table_op(cx, editor::TableOp::DuplicateCol);
    assert_eq!(all(cx), "id|name|name|score/3|Zoé|Zoé|10/1|Max|Max|/2|Léa|Léa|9");
    cx.simulate_keystrokes("secondary-z");
    table_op(cx, editor::TableOp::MoveColLeft);
    assert_eq!(all(cx), "id|score|name/3|10|Zoé/1||Max/2|9|Léa");
    assert_eq!(sheet.read_with(cx, |sheet, _| sheet.whereabouts().0), (0, 1));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(all(cx), "id|name|score/3|Zoé|10/1|Max|/2|Léa|9");
    // Moving stops at the edges: left of the first column, above the body.
    cx.simulate_keystrokes("secondary-home");
    table_op(cx, editor::TableOp::MoveColLeft);
    table_op(cx, editor::TableOp::MoveRowUp);
    assert_eq!(all(cx), "id|name|score/3|Zoé|10/1|Max|/2|Léa|9");
    // Move a row down; delete a column; refuse to delete them all.
    cx.simulate_keystrokes("down");
    table_op(cx, editor::TableOp::MoveRowDown);
    assert_eq!(all(cx), "id|name|score/1|Max|/3|Zoé|10/2|Léa|9");
    table_op(cx, editor::TableOp::DeleteCol);
    assert_eq!(all(cx), "name|score/Max|/Zoé|10/Léa|9");
    cx.simulate_keystrokes("secondary-a");
    table_op(cx, editor::TableOp::DeleteCol);
    assert_eq!(all(cx), "name|score/Max|/Zoé|10/Léa|9", "not every column can be deleted");
    cx.simulate_keystrokes("secondary-z secondary-z");
    assert_eq!(all(cx), "id|name|score/3|Zoé|10/1|Max|/2|Léa|9");
    assert_eq!(on_disk(cx), original);
    // Without a header row, the first row sorts like the others (numbers before text).
    sheet.update(cx, |sheet, cx| sheet.toggle_header(cx));
    cx.simulate_keystrokes("secondary-home");
    table_op(cx, editor::TableOp::SortAsc);
    assert_eq!(all(cx), "1|Max|/2|Léa|9/3|Zoé|10/id|name|score");
    cx.simulate_keystrokes("secondary-z");
    sheet.update(cx, |sheet, cx| sheet.toggle_header(cx));
    assert_eq!(all(cx), "id|name|score/3|Zoé|10/1|Max|/2|Léa|9");
    // A single row: sorting, or moving a row up and down, changes nothing and leaves no undo step;
    // a column can still go right, but not left of the first.
    let one = root.join("une.csv");
    fs::write(&one, "a,b\n").unwrap();
    shell.update(cx, |s, cx| {
        let (notes, dirs, images) = vault::scan(root);
        s.sync(notes, dirs, images, cx);
        s.open_note(&one, cx);
    });
    let sheet = shell.read_with(cx, |s, _| s.shown_sheet().expect("the table is shown"));
    let undone = sheet.read_with(cx, |sheet, _| sheet.undo_len());
    for op in [editor::TableOp::SortAsc, editor::TableOp::SortDesc, editor::TableOp::MoveRowUp, editor::TableOp::MoveRowDown, editor::TableOp::MoveColLeft] {
        sheet.update(cx, |sheet, cx| sheet.table_op(op, cx));
    }
    assert_eq!(sheet.read_with(cx, |sheet, _| sheet.undo_len()), undone);
    sheet.update(cx, |sheet, cx| sheet.table_op(editor::TableOp::MoveColRight, cx));
    assert_eq!(sheet.read_with(cx, |sheet, _| sheet.undo_len()), undone + 1);

    // Table: a Windows-1252 CSV separated by ";" opens in a grid, is edited and rewritten
    // in the same encoding, untouched rows byte for byte.
    let csv = root.join("data.csv");
    fs::write(&csv, b"nom;age\nAna\xefs;31\nBob;27\n").unwrap();
    shell.update(cx, |s, cx| {
        let (notes, dirs, images) = vault::scan(root);
        s.sync(notes, dirs, images, cx);
        s.open_note(&csv, cx);
    });
    let sheet = shell.read_with(cx, |s, _| s.shown_sheet().expect("the table is shown"));
    let format = sheet.read_with(cx, |sheet, _| sheet.format());
    assert_eq!((format.delimiter, format.encoding, format.header), (b';', table::Encoding::Windows1252, true));
    assert_eq!(sheet.read_with(cx, |sheet, _| sheet.table().cell(1, 0).to_string()), "Anaïs");
    // On first display the view is not measured yet, and GPUI does not redraw for a `notify`
    // made during drawing: the grid must already fill the window, not a few cells.
    let (lines, columns) = sheet.update(cx, |sheet, _| sheet.unmeasured());
    assert_eq!((lines, columns), (1..3, 0..2));
    shell.update_in(cx, |s, window, cx| {
        let sheet = s.shown_sheet().unwrap();
        window.focus(&sheet.focus_handle(cx));
    });
    let cell = |cx: &mut gpui::VisualTestContext, r, c| sheet.read_with(cx, |sheet, _| sheet.table().cell(r, c).to_string());
    // Typing replaces the cell, Enter submits and moves down.
    // The cell being written is an input line: word selected and replaced, space erased.
    let word = if cfg!(target_os = "macos") { "alt" } else { "ctrl" };
    cx.simulate_keystrokes("down right");
    cx.simulate_input("zéro 2");
    cx.simulate_keystrokes(&format!("home {word}-shift-right"));
    cx.simulate_input("3");
    cx.simulate_keystrokes("delete enter");
    assert_eq!(cell(cx, 1, 1), "32");
    // Shift + arrows selects a range; Ctrl+C copies it as TSV, Ctrl+V pastes it elsewhere.
    cx.simulate_keystrokes("up shift-left shift-down");
    cx.simulate_keystrokes("secondary-c");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("Anaïs\t32\nBob\t27"));
    cx.simulate_keystrokes("down");
    cx.simulate_keystrokes("secondary-v");
    // Pasted at the selection's corner (row 2), the grid adds the missing row.
    assert_eq!((cell(cx, 2, 0), cell(cx, 3, 1)), ("Anaïs".into(), "27".into()));
    assert_eq!(sheet.read_with(cx, |sheet, _| sheet.table().rows()), 4);
    // Undo reverts the paste in one go, added rows included.
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(sheet.read_with(cx, |sheet, _| sheet.table().rows()), 3);
    // Writing follows after a short delay; only the touched row changed.
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(fs::read(&csv).unwrap(), b"nom;age\nAna\xefs;32\nBob;27\n");
    // A Markdown copy pastes into a note; export creates a file next to the table.
    // On a single cell, they take the whole table.
    cx.simulate_keystrokes("secondary-home");
    sheet.update(cx, |sheet, cx| sheet.copy_as(table::Style::Markdown, cx));
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()).as_deref().map(|t| t.lines().next().unwrap_or("").to_string()),
        Some("| nom | age |".to_string())
    );
    sheet.update(cx, |sheet, cx| sheet.export(table::Style::Json, cx));
    assert!(root.join("data.json").is_file());
    // Ctrl+click: a second selection adds to the first. Copy takes both, one
    // under the other; clearing too, in a single undo; a plain click keeps only its own.
    cx.run_until_parked();
    let spot = |cx: &mut gpui::VisualTestContext, r, c| sheet.read_with(cx, |sheet, _| sheet.spot(r, c));
    let picked = |cx: &mut gpui::VisualTestContext| sheet.read_with(cx, |sheet, _| sheet.picked());
    let (first, second) = (spot(cx, 1, 0), spot(cx, 2, 1));
    cx.simulate_click(first, gpui::Modifiers::none());
    cx.simulate_click(second, gpui::Modifiers::secondary_key());
    assert_eq!(picked(cx), [(1..2, 0..1), (2..3, 1..2)]);
    cx.simulate_keystrokes("secondary-c");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("Anaïs\n27"));
    cx.simulate_keystrokes("delete");
    assert_eq!((cell(cx, 1, 0), cell(cx, 2, 1), cell(cx, 1, 1)), (String::new(), String::new(), "32".to_string()));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!((cell(cx, 1, 0), cell(cx, 2, 1)), ("Anaïs".to_string(), "27".to_string()));
    cx.simulate_click(first, gpui::Modifiers::none());
    assert_eq!(picked(cx), [(1..2, 0..1)]);
    // Ctrl+Space takes the whole column: Ctrl+Enter inserts one to its right, Ctrl+Delete removes it,
    // it and not all the rows. Shift+Space takes the row, which Ctrl+Shift+Backspace removes.
    let size = |cx: &mut gpui::VisualTestContext| sheet.read_with(cx, |sheet, _| (sheet.table().rows(), sheet.table().cols()));
    cx.simulate_keystrokes("ctrl-space");
    assert_eq!(picked(cx), [(0..3, 0..1)]);
    cx.simulate_keystrokes("secondary-enter");
    assert_eq!((size(cx), cell(cx, 0, 1), cell(cx, 0, 2)), ((3, 3), String::new(), "age".to_string()));
    assert_eq!(picked(cx), [(0..3, 1..2)]);
    cx.simulate_keystrokes("secondary-z");
    cx.simulate_click(first, gpui::Modifiers::none());
    cx.simulate_keystrokes("ctrl-space secondary-delete");
    assert_eq!((size(cx), cell(cx, 0, 0), cell(cx, 1, 0)), ((3, 1), "age".to_string(), "32".to_string()));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!((size(cx), cell(cx, 0, 0)), ((3, 2), "nom".to_string()));
    cx.simulate_click(first, gpui::Modifiers::none());
    cx.simulate_keystrokes("shift-space");
    assert_eq!(picked(cx), [(1..2, 0..2)]);
    cx.simulate_keystrokes("secondary-shift-backspace");
    assert_eq!((size(cx), cell(cx, 1, 0)), ((2, 2), "Bob".to_string()));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!((size(cx), cell(cx, 1, 0)), ((3, 2), "Anaïs".to_string()));
    // Selecting a column, a row or everything does not scroll the view (it does not jump to the last row).
    let long = root.join("long.csv");
    fs::write(&long, (0..400).map(|i| format!("{i};a{i};b{i};c{i}\n")).collect::<String>()).unwrap();
    shell.update(cx, |s, cx| {
        let (notes, dirs, images) = vault::scan(root);
        s.sync(notes, dirs, images, cx);
        s.open_note(&long, cx);
    });
    cx.run_until_parked();
    let sheet = shell.read_with(cx, |s, _| s.shown_sheet().unwrap());
    sheet.update(cx, |sheet, _| sheet.scroll_by(0., 2000.));
    let before = sheet.read_with(cx, |sheet, _| sheet.view_state().0);
    assert!(before.1 > 1000., "the view did scroll down: {before:?}");
    for (what, n, cells) in [("col", 2, (0..400, 2..3)), ("row", 300, (300..301, 0..4)), ("all", 0, (0..400, 0..4))] {
        sheet.update(cx, |sheet, cx| sheet.pick(what, n, cx));
        let (scroll, selected) = sheet.read_with(cx, |sheet, _| sheet.view_state());
        assert_eq!((scroll, selected), (before, cells), "{what}");
    }
    shell.update(cx, |s, cx| s.open_note(&csv, cx));
    cx.run_until_parked();
    let sheet = shell.read_with(cx, |s, _| s.shown_sheet().unwrap());

    // Changing the delimiter rereads the file that way, and the choice is remembered.
    sheet.update(cx, |sheet, cx| sheet.reformat(Some(table::Format { delimiter: b',', ..sheet.format() }), cx));
    assert_eq!(sheet.read_with(cx, |sheet, _| sheet.table().cols()), 1);
    assert_eq!(sheet.read_with(cx, |sheet, _| sheet.table().cell(0, 0).to_string()), "nom;age");
    shell.update(cx, |s, cx| {
        s.open_note(&root.join("Racine.md"), cx);
    });
    shell.update(cx, |s, cx| s.open_note(&csv, cx));
    let delimiter = shell.read_with(cx, |s, cx| s.shown_sheet().unwrap().read(cx).format().delimiter);
    assert_eq!(delimiter, b',');

    // In a note, selected table cells copy as a Markdown table, which pastes as a table in another note.
    shell.update_in(cx, |s, window, cx| {
        s.new_note("| a | b |\n| --- | --- |\n| 1 | 2 |\n".into(), cx);
        window.focus(&s.pane().editor.focus_handle(cx));
    });
    cx.simulate_keystrokes("secondary-a secondary-c");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("| a   | b   |\n| --- | --- |\n| 1   | 2   |"));
    shell.update_in(cx, |s, window, cx| {
        s.new_note("autre\n\n".into(), cx);
        window.focus(&s.pane().editor.focus_handle(cx));
    });
    cx.simulate_keystrokes("secondary-end secondary-v");
    let pasted = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string());
    assert_eq!(pasted, "autre\n\n| a   | b   |\n| --- | --- |\n| 1   | 2   |");
}
