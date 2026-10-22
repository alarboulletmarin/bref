//! Formulas in the tables of a note: results shown, formula under the cursor, errors, total, copy, completion.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    let shown = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).shown_table());
    // The table as the reader sees it, `a|b/c|d`.
    let seen = |cx: &mut gpui::VisualTestContext| shown(cx).iter().map(|r| r.join("|")).collect::<Vec<_>>().join("/");
    let offered = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).completions());
    let clipboard = |cx: &mut gpui::VisualTestContext| cx.read_from_clipboard().and_then(|c| c.text()).unwrap_or_default();

    let budget = "| Item | Qty | Price | Total |\n| --- | --- | --- | --- |\n| Coffee | 3 | 2,5 | =B*C |\n| Cake | 2 | 4 € | =B*C |\n| Sum |  |  | =sum(above) |\n\nsuite";
    // The results are shown, the file keeps the formulas; French decimal sign in the tests.
    j.load(cx, budget, 0);
    assert_eq!(seen(cx), "Item|Qty|Price|Total/Coffee|3|2,5|7,5/Cake|2|4 €|8/Sum|||15,5");
    assert_eq!(text(cx), budget);
    // The cursor in a formula shows it, and leaves it when it moves away; the others keep their result.
    j.load(cx, budget, budget.find("=B*C").unwrap() + 2);
    assert_eq!(seen(cx), "Item|Qty|Price|Total/Coffee|3|2,5|=B*C/Cake|2|4 €|8/Sum|||15,5");
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    let caret = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).caret());
    assert_eq!(seen(cx), "Item|Qty|Price|Total/Coffee|3|2,5|7,5/Cake|2|4 €|=B*C/Sum|||15,5", "caret {caret} in {:?}", &budget[caret.min(budget.len())..]);
    // Results follow what is typed: a quantity changes the totals.
    j.load(cx, budget, budget.find("| 3 |").unwrap() + 2);
    cx.simulate_keystrokes("shift-right");
    cx.simulate_input("10");
    assert_eq!(seen(cx), "Item|Qty|Price|Total/Coffee|10|2,5|25/Cake|2|4 €|8/Sum|||33");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(seen(cx), "Item|Qty|Price|Total/Coffee|3|2,5|7,5/Cake|2|4 €|8/Sum|||15,5");

    // Errors: `#ERR`, and the reason when the pointer is on the cell.
    let broken = "| a | b |\n| - | - |\n| 1 | =1/0 |\n| 2 | =foo(1) |\n| 3 | =B3 |\n| 4 | =A1+ |\n| 5 | =Z9 |\n| t | =A6+1 |\n\nsuite";
    j.load(cx, broken, 0);
    assert_eq!(seen(cx), "a|b/1|#ERR/2|#ERR/3|#ERR/4|#ERR/5|#ERR/t|#ERR");
    for (row, reason) in [(1, "division par zéro"), (2, "fonction ou nom inconnu : foo"), (3, "la cellule dépend d'elle-même"), (4, "la formule s'arrête trop tôt"), (5, "référence hors du tableau : Z9"), (6, "pas un nombre : “t”")] {
        let at = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).cell_point(row, 1).unwrap());
        cx.simulate_mouse_move(at, None, gpui::Modifiers::none());
        assert_eq!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).hint()).as_deref(), Some(reason), "row {row}");
    }
    let off = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).cell_point(1, 0).unwrap());
    cx.simulate_mouse_move(off, None, gpui::Modifiers::none());
    assert_eq!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).hint()), None, "no reason on a cell that computes");
    // The total: above, or on the left where only those hold numbers; never over a cell with something in it.
    let sums = "| a | b | c |\n| - | - | - |\n| 1 | 2 |  |\n| 3 | 4 |  |\n| x |  |  |\n|  | y | z |";
    // Start of cell `c` of line `l`.
    let cell_at = |table: &str, l: usize, c: usize| {
        let start: usize = table.split('\n').take(l).map(|x| x.len() + 1).sum();
        start + markdown::cells(table.split('\n').nth(l).unwrap())[c].start
    };
    j.load(cx, sums, cell_at(sums, 4, 1));
    cx.simulate_keystrokes("alt-=");
    assert!(text(cx).contains("| x   | =sum(above) |"), "{:?}", text(cx));
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_eq!(seen(cx), "a|b|c/1|2|/3|4|/x|6|/|y|z");
    cx.simulate_keystrokes("secondary-z");
    j.load(cx, sums, cell_at(sums, 2, 2));
    cx.simulate_keystrokes("alt-=");
    assert!(text(cx).contains("=sum(left)"), "{:?}", text(cx));
    cx.simulate_keystrokes("secondary-z");
    for (line, col) in [(4, 0), (0, 1), (1, 1), (5, 1)] {
        j.load(cx, sums, cell_at(sums, line, col));
        cx.simulate_keystrokes("alt-=");
        assert_eq!(text(cx), sums, "nothing over a cell with something in it, the header or the dashes: ({line}, {col})");
    }
    // Copy: the cells as they are (Markdown, formulas kept, they paste back as formulas); the results for a spreadsheet.
    j.load(cx, budget, 0);
    cx.run_until_parked();
    let corner = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).part_center(editor::Part::Corner).unwrap());
    cx.simulate_click(corner, gpui::Modifiers::none());
    cx.simulate_keystrokes("secondary-c");
    assert!(clipboard(cx).contains("=B*C") && clipboard(cx).contains("=sum(above)"), "{:?}", clipboard(cx));
    cx.simulate_keystrokes("secondary-alt-c");
    assert_eq!(clipboard(cx), "Item\tQty\tPrice\tTotal\nCoffee\t3\t2,5\t7,5\nCake\t2\t4 €\t8\nSum\t\t\t15,5");
    // A column taken by its grip: only its cells.
    let col = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).part_center(editor::Part::Col(3)).unwrap());
    cx.simulate_click(col, gpui::Modifiers::none());
    cx.simulate_keystrokes("secondary-alt-c");
    assert_eq!(clipboard(cx), "Total\n7,5\n8\n15,5");
    // Without a grip or a selection, the table at the cursor; outside a table, the clipboard stays as it was.
    j.load(cx, "dehors\n\n| a | b |\n| - | - |\n| =1+1 | 2 |", 0);
    cx.simulate_keystrokes("secondary-alt-c");
    assert_eq!(clipboard(cx), "Total\n7,5\n8\n15,5");

    // Completion after `=`: functions, ranges, columns by letter or header; nothing for what is written whole, nor outside a formula.
    let table = "| Item | Qty | Total |\n| --- | --- | --- |\n| a | 3 | =s |";
    j.load(cx, table, table.len() - 2);
    assert_eq!(offered(cx), ["Somme des nombres sum("]);
    cx.simulate_keystrokes("enter");
    assert!(text(cx).ends_with("| =sum( |"), "{:?}", text(cx));
    j.load(cx, table, table.len() - 2);
    cx.simulate_keystrokes("backspace");
    cx.simulate_input("q");
    assert!(offered(cx).is_empty(), "a header is looked for from two letters on");
    cx.simulate_input("ty");
    assert_eq!(offered(cx), ["Qty B"]);
    cx.simulate_keystrokes("enter");
    assert!(text(cx).contains("| =B "), "{:?}", text(cx));
    j.load(cx, "| Item | Qty | Total |\n| --- | --- | --- |\n| a | 3 | =sum(above) |", 0);
    assert!(offered(cx).is_empty());
    for (formula, at, want) in [
        ("=ab", 3, vec!["Valeur absolue abs(", "Les cellules au-dessus above"]),
        ("=c", 2, vec!["Combien de nombres count(", "Toute la colonne column"]),
        ("=1+b", 4, vec![]),
        ("=1+qty", 6, vec!["Qty B"]),
        ("=b2", 3, vec![]),
        ("=3x", 3, vec![]),
        ("=B", 2, vec![]),
        ("=", 1, vec![]),
        ("=if(a", 5, vec!["Moyenne avg(", "Valeur absolue abs(", "Les cellules au-dessus above"]),
    ] {
        let table = format!("| Item | Qty | Total |\n| --- | --- | --- |\n| a | 3 | {formula} |");
        j.load(cx, &table, table.len() - 2 - (formula.len() - at));
        assert_eq!(offered(cx), want, "{formula}");
    }
    // A `/` in a formula is a division, not a command; outside one, the command menu opens as before.
    let divide = "| a | b |\n| - | - |\n| 4 | =A /";
    j.load(cx, divide, divide.len());
    assert!(offered(cx).is_empty(), "{:?}", offered(cx));
    j.load(cx, "| a | b |\n| - | - |\n| 4 | x /", 30);
    j.load(cx, "/h", 2);
    assert!(!offered(cx).is_empty());

    // A highlight in a cell is text, not a formula: no `#ERR`, with the cursor in it or not, in the header, among numbers.
    let marked = "| ==titre== | b |\n| - | - |\n| ==faux== | ==a== b |\n| == a == | ==faux |\n| === | {==note==}{>>x<<} |\n| =2*3 | =sum(above) |";
    j.load(cx, marked, 0);
    assert_eq!(seen(cx), "==titre==|b/==faux==|==a== b/== a ==|==faux/===|{==note==}{>>x<<}/6|0", "{:?}", seen(cx));
    assert_eq!(text(cx), marked);
    j.load(cx, marked, marked.find("==faux==").unwrap() + 4);
    assert!(!seen(cx).contains("#ERR"), "{:?}", seen(cx));
    // Typing `=` then `=` again: a formula until the second sign, text after it; undo gives the formula back.
    let typed = "| a |\n| - |\n| 1 |\n| = |";
    j.load(cx, typed, typed.len() - 2);
    cx.simulate_input("=faux==");
    assert!(text(cx).contains("| ==faux== |") && !seen(cx).contains("#ERR"), "{:?} {:?}", text(cx), seen(cx));

    // Limits. Numbers from 0 to a huge one, negative, with the French sign; a lone `=` is text; a header can compute;
    // an indented table; text results with CJK and emoji; a note without a table is left alone.
    let values = "| v |\n| - |\n| =0 |\n| =1 |\n| =9 |\n| =10 |\n| =99 |\n| =100 |\n| =1000 |\n| =100000 |\n| =-5 |\n| =1e |\n| =2^60 |\n| =0.5 |\n| =-0.05 |\n| = |";
    j.load(cx, values, 0);
    assert_eq!(seen(cx), "v/0/1/9/10/99/100/1000/100000/-5/#ERR/1,152921504606847e18/0,5/-0,05/=", "{:?}", seen(cx));
    j.load(cx, "| =1+1 | b |\n| - | - |\n| 1 | 2 |", 0);
    assert_eq!(seen(cx), "2|b/1|2");
    j.load(cx, "  | a | b |\n  | - | - |\n  | 1 | =A+1 |", 0);
    assert_eq!(seen(cx), "a|b/1|2");
    j.load(cx, "| a | b |\n| - | - |\n| 1 | =if(A, \"日本😀é\", \"\") |", 0);
    assert_eq!(seen(cx), "a|b/1|日本😀é");
    j.load(cx, "| a |\n| - |\n| 1 |\n\n=1+1 hors tableau", 0);
    assert_eq!(seen(cx), "a/1");
    // The first line of a table with a thousand lines and a column of formulas, and one past the limit: computed or not,
    // the text is intact and nothing breaks.
    for n in [1000, grid::BIG + 10] {
        let long = format!("| a | b |\n| - | - |\n{}", "| 1 | =A+1 |\n".repeat(n));
        j.load(cx, &long, 0);
        cx.run_until_parked();
        assert_eq!(text(cx), long);
    }
    let many = format!("| a | b | c |\n| - | - | - |\n{}", (0..300).map(|i| format!("| {i} | {i} | =sum(left) |\n")).collect::<String>());
    j.load(cx, &many, 0);
    cx.run_until_parked();
    let last = shown(cx).last().cloned().unwrap();
    assert_eq!(last, ["299", "299", "598"], "{last:?}");
}
