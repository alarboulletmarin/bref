//! Styles and comments on cells: a selection or a handle that takes several cells styles each of them, never across a `|`.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // The rows of the table, cells trimmed, as `a|b/c|d`.
    let flat = |cx: &mut gpui::VisualTestContext| {
        text(cx).lines().filter(|l| l.starts_with('|') && !l.contains("---")).map(|l| markdown::cells(l).into_iter().map(|r| l[r].to_string()).collect::<Vec<_>>().join("|")).collect::<Vec<_>>().join("/")
    };
    // Every line of the table has the same width: the table stayed aligned.
    let aligned = |cx: &mut gpui::VisualTestContext| {
        let t = text(cx);
        let w: Vec<usize> = t.lines().filter(|l| l.starts_with('|')).map(|l| l.chars().count()).collect();
        w.windows(2).all(|p| p[0] == p[1])
    };
    let center = |cx: &mut gpui::VisualTestContext, part: editor::Part| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).part_center(part).unwrap());
    let styles = |cx: &mut gpui::VisualTestContext| shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, _| e.styles()));
    let none = gpui::Modifiers::none();
    use editor::Part;
    let grid = "| Nom | Âge |\n| --- | --- |\n| Léa | 31  |\n| Max | 7   |\n\nsuite";
    let start = |cx: &mut gpui::VisualTestContext, table: &str| {
        j.load(cx, table, table.find("Léa").unwrap_or(0));
        cx.run_until_parked();
    };

    // A column taken by its handle: every cell, the header included; the cursor stays in the first
    // cell, the column stays taken, so the same gesture takes the style off; one undo undoes each.
    start(cx, grid);
    let at = center(cx, Part::Col(1));
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(flat(cx), "Nom|**Âge**/Léa|**31**/Max|**7**");
    assert!(aligned(cx), "{:?}", text(cx));
    assert_eq!(styles(cx) & markdown::BOLD, markdown::BOLD, "the bar lights the button when every cell has it");
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(flat(cx), "Nom|Âge/Léa|31/Max|7");
    assert!(aligned(cx));
    cx.simulate_keystrokes("secondary-b");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(flat(cx), "Nom|Âge/Léa|31/Max|7", "one undo for the whole gesture");
    // A row; the whole table; a different style on each.
    cx.run_until_parked();
    let at = center(cx, Part::Row(2));
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-i");
    assert_eq!(flat(cx), "Nom|Âge/Léa|31/*Max*|*7*");
    let at = center(cx, Part::Corner);
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-shift-x");
    assert_eq!(flat(cx), "~~Nom~~|~~Âge~~/~~Léa~~|~~31~~/~~*Max*~~|~~*7*~~");
    assert!(aligned(cx));
    // Text selected across two rows: each cell touched, the `|` untouched; the second row is still a row.
    start(cx, grid);
    cx.simulate_keystrokes("shift-down shift-right shift-right shift-right");
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(flat(cx), "Nom|Âge/**Léa**|**31**/**Max**|7", "{:?}", text(cx));
    assert!(aligned(cx));
    // The selection followed its text: the same gesture takes it off.
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(flat(cx), "Nom|Âge/Léa|31/Max|7");
    // Some cells have it already: it goes where it is missing, and a second gesture removes it everywhere.
    let marked = "| Nom | Âge |\n| --- | --- |\n| **Léa** | 31  |\n| Max | 7   |";
    j.load(cx, marked, 2);
    cx.run_until_parked();
    let at = center(cx, Part::Corner);
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(flat(cx), "**Nom**|**Âge**/**Léa**|**31**/**Max**|**7**", "{:?}", text(cx));
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(flat(cx), "Nom|Âge/Léa|31/Max|7");
    // Empty cells are skipped: no `****` is left behind. The corner takes them all.
    let holes = "| a |  |\n| - | - |\n|  | d |";
    j.load(cx, holes, 2);
    cx.run_until_parked();
    let at = center(cx, Part::Corner);
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(flat(cx), "**a**|/|**d**", "{:?}", text(cx));
    assert!(aligned(cx));
    // Highlight and code go cell by cell too; escaped bars and accents stay whole.
    let odd = "| x \\| y | é |\n| - | - |\n| 日本 | 😀 |";
    j.load(cx, odd, 2);
    cx.run_until_parked();
    let at = center(cx, Part::Corner);
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-shift-h");
    assert_eq!(flat(cx), "==x \\| y==|==é==/==日本==|==😀==", "{:?}", text(cx));
    cx.simulate_keystrokes("secondary-shift-h");
    cx.simulate_keystrokes("secondary-shift-k");
    assert_eq!(flat(cx), "`x \\| y`|`é`/`日本`|`😀`", "{:?}", text(cx));
    // Several cursors on the `==` of several cells (each signs pair taken by Ctrl+D), then Delete:
    // each cursor removes its own signs, though the first one realigns the table under the others.
    let signs = "| a | b |\n| --- | --- |\n| ==Léa== | ==31== |\n| ==Max== | 7 |";
    j.load(cx, signs, signs.find("==").unwrap());
    cx.run_until_parked();
    cx.simulate_keystrokes("shift-right shift-right secondary-d secondary-d secondary-d secondary-d secondary-d delete");
    assert_eq!(flat(cx), "a|b/Léa|31/Max|7", "{:?}", text(cx));
    assert!(aligned(cx));
    // The cursors stayed where the signs were: Backspace takes the last letter of each text, never a `|`.
    cx.simulate_keystrokes("backspace");
    assert_eq!(flat(cx), "a|b/Lé|3/Ma|7", "{:?}", text(cx));
    cx.simulate_keystrokes("secondary-z secondary-z");
    assert_eq!(text(cx), signs);
    // A selection that starts in a cell and ends in the text after the table: the table by cell, the text as usual.
    start(cx, grid);
    cx.simulate_keystrokes("shift-down shift-down shift-down shift-down shift-down shift-right shift-right");
    cx.simulate_keystrokes("secondary-b");
    let t = text(cx);
    assert!(t.contains("**suite**") || t.contains("**su**ite"), "{t:?}");
    assert!(t.lines().filter(|l| l.starts_with('|')).all(|l| l.matches('|').count() == 3), "{t:?}");
    // A cursor alone in a cell: its signs go in the cell, the table stays aligned and the cursor between them.
    start(cx, grid);
    cx.simulate_keystrokes("secondary-b");
    cx.simulate_input("x");
    assert_eq!(flat(cx), "Nom|Âge/**x**Léa|31/Max|7", "{:?}", text(cx));
    assert!(aligned(cx));

    // Comments. A cursor in a cell comments the cell's text, the row stays a row; resolving
    // keeps the text; a `|` typed in the comment is written `\|`.
    start(cx, grid);
    cx.simulate_keystrokes("secondary-shift-m");
    cx.simulate_input("a|b");
    assert_eq!(flat(cx), "Nom|Âge/{==Léa==}{>>a\\|b<<}|31/Max|7", "{:?}", text(cx));
    assert!(aligned(cx));
    assert_eq!(markdown::all_comments(&text(cx)).len(), 1);
    cx.simulate_keystrokes("secondary-shift-m");
    assert_eq!(flat(cx), "Nom|Âge/Léa|31/Max|7");
    cx.simulate_keystrokes("secondary-z");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), grid, "typing and the comment are undone");
    // A selection over two rows: a comment on each cell it touches, the second row stays a row.
    cx.simulate_keystrokes("shift-down shift-right shift-right shift-right");
    cx.simulate_keystrokes("secondary-shift-m");
    assert_eq!(flat(cx), "Nom|Âge/{==Léa==}{>><<}|{==31==}{>><<}/{==Max==}{>><<}|7", "{:?}", text(cx));
    assert!(aligned(cx));
    // The command on the dash line, or on an empty cell, does nothing and breaks nothing.
    for (table, at) in [(grid, grid.find("---").unwrap()), (holes_table(), 6)] {
        j.load(cx, table, at);
        cx.simulate_keystrokes("secondary-shift-m");
        assert_eq!(text(cx), table);
    }
    // A column, a row, the table, taken by their handles: one line under the table, named by content.
    start(cx, grid);
    let at = center(cx, Part::Col(1));
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-shift-m");
    cx.simulate_input("en années");
    assert_eq!(text(cx), format!("{}\n\n{{>>Column \"Âge\": en années<<}}\n\nsuite", grid.trim_end_matches("\n\nsuite")));
    let note = text(cx);
    let said = markdown::all_comments(&note);
    assert_eq!((said[0].noted, said[0].scope.as_str(), said[0].said), ("", "Column Âge", "en années"));
    cx.simulate_keystrokes("secondary-shift-m");
    assert_eq!(text(cx), grid, "resolving a comment on its own removes its line");
    start(cx, grid);
    let at = center(cx, Part::Row(2));
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-shift-m");
    assert!(text(cx).contains("\n\n{>>Row \"Max\": <<}\n\nsuite"), "{:?}", text(cx));
    cx.simulate_keystrokes("secondary-z");
    let at = center(cx, Part::Corner);
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-shift-m");
    assert!(text(cx).contains("\n\n{>>Table: <<}\n\nsuite") && flat(cx) == "Nom|Âge/Léa|31/Max|7", "{:?}", text(cx));
    // Two rows with the same first cell are told apart; a table at the end of the note, without a line break.
    let twins = "| a | b |\n| - | - |\n| x | 1 |\n| x | 2 |";
    j.load(cx, twins, 2);
    cx.run_until_parked();
    let at = center(cx, Part::Row(2));
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-shift-m");
    assert_eq!(text(cx), format!("{twins}\n\n{{>>Row \"x\" (2): <<}}"));
    // The comments under a table follow it. Renamed first cell or header: the name changes in the
    // comment; a sort leaves it with its row; deleting the row, the column or the table deletes it,
    // in one undo step.
    let note = "| Nom | Âge |\n| --- | --- |\n| Léa | 31  |\n| Max | 7   |\n\n{>>Row \"Léa\": a<<}\n\n{>>Column \"Âge\": b<<}\n\n{>>Table: c<<}\n\n{>>libre<<}\n\nsuite";
    let comments = |cx: &mut gpui::VisualTestContext| text(cx).lines().filter(|l| l.starts_with("{>>")).collect::<Vec<_>>().join(" ");
    j.load(cx, note, note.find("Léa").unwrap());
    cx.simulate_keystrokes("shift-right shift-right shift-right");
    cx.simulate_input("Zoé");
    assert_eq!(comments(cx), "{>>Row \"Zoé\": a<<} {>>Column \"Âge\": b<<} {>>Table: c<<} {>>libre<<}", "{:?}", text(cx));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), note, "typing and the renaming are one undo");
    j.load(cx, note, note.find("Léa").unwrap());
    cx.simulate_keystrokes("shift-right shift-right shift-right");
    cx.simulate_keystrokes("backspace");
    assert!(comments(cx).starts_with("{>>Row 2: a<<}"), "{}", comments(cx));
    j.load(cx, note, note.find("Nom").unwrap());
    cx.simulate_keystrokes("shift-right shift-right shift-right");
    cx.simulate_input("Prénom");
    assert!(comments(cx).contains("{>>Column \"Âge\": b<<}"), "another header leaves the column alone");
    j.load(cx, note, note.find("Âge").unwrap());
    cx.simulate_keystrokes("shift-right shift-right shift-right");
    cx.simulate_input("Années");
    assert!(comments(cx).contains("{>>Column \"Années\": b<<}") && comments(cx).contains("{>>Row \"Léa\": a<<}"), "{}", comments(cx));
    j.load(cx, note, note.find("Léa").unwrap());
    cx.run_until_parked();
    let op = |cx: &mut gpui::VisualTestContext, op: editor::TableOp| {
        shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Table(op), None, window, cx));
    };
    op(cx, editor::TableOp::SortDesc);
    assert_eq!(flat(cx), "Nom|Âge/Max|7/Léa|31");
    assert!(comments(cx).starts_with("{>>Row \"Léa\": a<<}"), "a sort leaves the comment with its row");
    j.load(cx, note, note.find("Léa").unwrap());
    cx.run_until_parked();
    op(cx, editor::TableOp::DeleteRow);
    assert_eq!(comments(cx), "{>>Column \"Âge\": b<<} {>>Table: c<<} {>>libre<<}", "{:?}", text(cx));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), note, "one undo brings back the row and its comment");
    let at = center(cx, Part::Col(1));
    cx.simulate_click(at, none);
    op(cx, editor::TableOp::DeleteCol);
    assert_eq!(comments(cx), "{>>Row \"Léa\": a<<} {>>Table: c<<} {>>libre<<}", "{:?}", text(cx));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), note);
    op(cx, editor::TableOp::DeleteTable);
    assert_eq!(comments(cx), "{>>libre<<}", "{:?}", text(cx));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), note);
    // Emptying cells renames too: a cleared column and a pasted header carry their comments along; and a
    // link over several cells is refused (it would hold a `|`).
    j.load(cx, note, note.find("Léa").unwrap());
    cx.run_until_parked();
    let at = center(cx, Part::Col(1));
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("delete");
    assert!(comments(cx).contains("{>>Column 2: b<<}") && comments(cx).contains("{>>Row \"Léa\": a<<}"), "{}", comments(cx));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), note, "one undo for the clearing and the renaming");
    cx.run_until_parked();
    let at = center(cx, Part::Col(0));
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("delete");
    assert!(comments(cx).starts_with("{>>Row 3: a<<}") || comments(cx).starts_with("{>>Row 2: a<<}"), "{}", comments(cx));
    cx.simulate_keystrokes("secondary-z");
    let at = center(cx, Part::Corner);
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-k");
    assert_eq!(text(cx), note, "no link on a whole table");
    j.load(cx, note, note.find("Léa").unwrap());
    cx.simulate_keystrokes("shift-down shift-right");
    cx.simulate_keystrokes("secondary-k");
    assert_eq!(text(cx), note, "no link across two rows");
    j.load(cx, "| a | b |\n| - | - |\n| mot | y |", 22);
    cx.simulate_keystrokes("shift-right shift-right shift-right");
    cx.simulate_keystrokes("secondary-k");
    assert!(text(cx).contains("[mot]()"), "a link on one cell still works: {:?}", text(cx));
    // Duplicated, the row keeps its comment on the original, now told from its copy; swapping rows and
    // columns turns the comments of the rows into those of the columns, and the other way round.
    j.load(cx, note, note.find("Léa").unwrap());
    cx.run_until_parked();
    op(cx, editor::TableOp::DuplicateRow);
    assert!(comments(cx).starts_with("{>>Row \"Léa\" (1): a<<} {>>Column \"Âge\": b<<}"), "{}", comments(cx));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), note);
    j.load(cx, note, note.find("Léa").unwrap());
    cx.run_until_parked();
    op(cx, editor::TableOp::Transpose);
    assert_eq!(comments(cx), "{>>Column \"Léa\": a<<} {>>Row \"Âge\": b<<} {>>Table: c<<} {>>libre<<}", "{:?}", text(cx));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), note);
    // A row inserted above does not take the comment of the one that was there.
    j.load(cx, note, note.find("Léa").unwrap());
    cx.run_until_parked();
    op(cx, editor::TableOp::RowAbove);
    assert!(comments(cx).starts_with("{>>Row \"Léa\": a<<}"), "{}", comments(cx));
    // Deleting a row that is not commented leaves the comment of another row where it is, and the
    // rows that shared a first cell lose their number.
    let twins = "| a | b |\n| - | - |\n| x | 1 |\n| x | 2 |\n\n{>>Row \"x\" (2): n<<}";
    j.load(cx, twins, twins.find('1').unwrap());
    cx.run_until_parked();
    op(cx, editor::TableOp::DeleteRow);
    assert_eq!(comments(cx), "{>>Row \"x\": n<<}", "{:?}", text(cx));
    // What is drawn: the parts that carry a comment, and the one the cursor is in the comment of.
    use editor::Target::*;
    let notes = |cx: &mut gpui::VisualTestContext| shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, _| e.first_notes()));
    let marked = "| Nom | {==Âge==}{>>a<<} | Ville |\n| --- | --- | --- |\n| Léa | 31 | {==Lyon==}{>>b<<} |\n| Max | {==7==}{>>c<<} é | x |\n\n{>>Row \"Max\": d<<}\n\n{>>Column \"Ville\": e<<}\n\n{>>Table: f<<}\n\n{>>Row 9: g<<}\n\nsuite";
    j.load(cx, marked, 0);
    cx.run_until_parked();
    let n = notes(cx);
    // `7 é` is more than the comment: the words only are highlighted, the cell is not tinted.
    assert_eq!(n.found, [Row(2), Col(2), Table, Cell(0, 1), Cell(1, 2)], "{n:?}");
    assert_eq!(n.active, None);
    for (at, want) in [("Row \"Max\": d", Row(2)), ("Column \"Ville\": e", Col(2)), ("Table: f", Table), ("Lyon", Cell(1, 2)), ("Âge", Cell(0, 1))] {
        j.load(cx, marked, marked.find(at).unwrap() + 2);
        cx.run_until_parked();
        assert_eq!(notes(cx).active, Some(want), "cursor in {at:?}");
    }
    j.load(cx, marked, marked.find("suite").unwrap());
    cx.run_until_parked();
    assert_eq!(notes(cx).active, None);
    // No comment, no mark; an orphan line (`Row 9`) marks nothing.
    j.load(cx, grid, 0);
    cx.run_until_parked();
    assert!(notes(cx).found.is_empty());
    // A comment on its own: resolving takes its line, at the start of the note, between two
    // paragraphs, at the end; in the middle of a line, only its markup goes.
    for (note, at, left) in [("{>>a<<}\nsuite", 3, "suite"), ("avant\n\n{>>a<<}\n\nsuite", 10, "avant\n\nsuite"), ("avant\n\n{>>a<<}", 10, "avant"), ("un {>>a<<} deux", 6, "un  deux")] {
        j.load(cx, note, at);
        cx.simulate_keystrokes("secondary-shift-m");
        assert_eq!(text(cx), left, "{note:?}");
    }
}

fn holes_table() -> &'static str {
    "| a |  |\n| - | - |\n|  | d |"
}
