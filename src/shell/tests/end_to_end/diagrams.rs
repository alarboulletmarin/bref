//! The diagram canvas, `![[` completion and `/` commands.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Diagram: Ctrl+Shift+D creates one next to the note. A letter picks the
    // shape, dragging places it; Enter writes in it; an arrow dragged from one
    // shape to the other attaches to them.
    cx.simulate_keystrokes("secondary-shift-d");
    cx.run_until_parked();
    let (file, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
    assert_eq!(file.file_name().unwrap(), "Schéma.svg");
    let drag = |cx: &mut gpui::VisualTestContext, from: (f32, f32), to: (f32, f32)| {
        let (from, to) = canvas.read_with(cx, |c, _| (c.spot(from.0, from.1), c.spot(to.0, to.1)));
        cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    };
    let saved = || Diagram::from_svg(&fs::read_to_string(&file).unwrap()).unwrap();
    cx.simulate_input("r");
    drag(cx, (0., 0.), (120., 60.));
    cx.simulate_keystrokes("enter");
    cx.simulate_input("Client");
    cx.simulate_keystrokes("escape");
    cx.simulate_input("o");
    drag(cx, (300., 0.), (400., 80.));
    cx.simulate_input("a");
    drag(cx, (60., 30.), (350., 40.));
    let drawn = saved();
    assert_eq!((drawn.shapes.len(), drawn.shapes[0].text.as_str(), drawn.shapes[0].w), (2, "Client", 120.));
    let ends = (diagram::End::Shape(drawn.shapes[0].id), diagram::End::Shape(drawn.shapes[1].id));
    assert_eq!((drawn.links[0].from, drawn.links[0].to, drawn.links[0].end), (ends.0, ends.1, diagram::Head::Arrow));
    // Dragged from a shape's edge, the arrow sticks there; it is elbowed by default.
    cx.simulate_input("a");
    drag(cx, (115., 30.), (350., 40.));
    let pinned = saved().links[1].clone();
    assert_eq!((pinned.from, pinned.route), (diagram::End::Pin(drawn.shapes[0].id, 1., 0.5), diagram::Route::Elbow));
    // "+" and "-" zoom, "0" fits everything.
    let span = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| f32::from(c.spot(100., 0.).x - c.spot(0., 0.).x));
    cx.simulate_input("+");
    assert_eq!(span(cx), 125.);
    cx.simulate_input("-");
    cx.simulate_input("0");
    cx.run_until_parked();
    assert!(span(cx) <= 100.);
    // A shape moves by dragging it, on the grid; Ctrl+Z puts it back.
    drag(cx, (350., 40.), (350., 143.));
    assert_eq!(saved().shapes[1].y, 100.);
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(saved().shapes[1].y, 0.);
    // Ctrl+click adds a shape to the selection; a second one removes it.
    let click = |cx: &mut gpui::VisualTestContext, at: (f32, f32), held: gpui::Modifiers| {
        let at = canvas.read_with(cx, |c, _| c.spot(at.0, at.1));
        cx.simulate_click(at, held);
    };
    click(cx, (20., 50.), gpui::Modifiers::none());
    click(cx, (350., 40.), gpui::Modifiers::secondary_key());
    assert_eq!(canvas.read_with(cx, |c, _| c.picked()), 2);
    click(cx, (350., 40.), gpui::Modifiers::secondary_key());
    assert_eq!(canvas.read_with(cx, |c, _| c.picked()), 1);
    // Delete removes the selected shape, and the arrow attached to it.
    drag(cx, (350., 40.), (350., 40.));
    cx.simulate_keystrokes("delete");
    assert_eq!((saved().shapes.len(), saved().links.len()), (1, 0));
    // The diagram is among the vault's images; Escape gives control back to the note.
    assert!(shell.read_with(cx, |s, _| s.images.contains(&file)));
    cx.simulate_keystrokes("escape escape");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.pane().file.is_none() && s.pane().drawing().is_none()));

    // `![[` offers the vault's images and diagrams; Enter completes the name.
    cx.simulate_keystrokes("secondary-end enter");
    cx.simulate_input("![[sch");
    cx.simulate_keystrokes("enter");
    assert!(text(cx).ends_with("![[Schéma.svg]]"));
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();

    // `/` commands: the list filters by name or label; Enter inserts the
    // component. Here a callout, whose quote continues on the next line.
    cx.simulate_keystrokes("enter");
    cx.simulate_input("/pan");
    cx.simulate_keystrokes("down enter");
    cx.simulate_input("vu");
    assert!(text(cx).ends_with("\n> [!TIP]\n> vu"));
    cx.simulate_keystrokes("enter enter");
    // `/tableau` opens the grid: arrows set columns and rows.
    cx.simulate_input("/tableau");
    cx.simulate_keystrokes("enter left down enter");
    cx.simulate_input("Nom");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("Âge");
    // Tab goes to the next cell, over the dashes, and realigns the columns.
    cx.simulate_keystrokes("tab");
    cx.simulate_input("Élodie");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("31");
    cx.simulate_keystrokes("tab");
    let empty = "|        |     |";
    let table = format!("| Nom    | Âge |\n| ------ | --- |\n| Élodie | 31  |\n{empty}\n{empty}");
    assert!(text(cx).ends_with(&format!("\n\n{table}")), "{}", text(cx));
    // Enter adds a row under the cursor's; the "+" buttons a
    // column on the right and a row at the bottom.
    cx.simulate_keystrokes("enter");
    assert!(text(cx).ends_with(&format!("{table}\n{empty}")));
    cx.run_until_parked();
    let [_, column] = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).plus_buttons().unwrap());
    cx.simulate_click(column, gpui::Modifiers::none());
    cx.simulate_input("Ville");
    // Each keystroke realigns the columns; erasing too.
    assert!(text(cx).contains("| Nom    | Âge | Ville |\n| ------ | --- | ----- |\n| Élodie | 31  |       |"), "{}", text(cx));
    cx.simulate_input("s x");
    cx.simulate_keystrokes("backspace backspace backspace");
    assert!(text(cx).contains("| Nom    | Âge | Ville |\n| ------ | --- | ----- |\n| Élodie | 31  |       |"), "{}", text(cx));
    cx.run_until_parked();
    let [row, _] = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).plus_buttons().unwrap());
    cx.simulate_click(row, gpui::Modifiers::none());
    // Enter on an empty last row removes it and leaves the table.
    cx.simulate_keystrokes("enter");
    cx.simulate_input("fin");
    let empty = "|        |     |       |";
    let table = format!("| Nom    | Âge | Ville |\n| ------ | --- | ----- |\n| Élodie | 31  |       |\n{empty}\n{empty}\n{empty}");
    assert!(text(cx).ends_with(&format!("\n\n{table}\nfin")), "{}", text(cx));
    // With the mouse: the hovered cell of the grid gives the table's size.
    cx.simulate_keystrokes("enter");
    cx.simulate_input("/table");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let cell = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).grid_cell(2, 1).unwrap());
    cx.simulate_click(cell, gpui::Modifiers::none());
    assert!(text(cx).ends_with("fin\n\n|     |     |\n| --- | --- |"), "{}", text(cx));
    // With the keyboard: a size typed in the grid, beyond its cells.
    cx.simulate_keystrokes("secondary-end enter enter");
    cx.simulate_input("/table");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("12x9");
    cx.simulate_keystrokes("backspace");
    cx.simulate_input("1");
    cx.simulate_keystrokes("enter");
    let wide = format!("|{}\n|{}", "     |".repeat(12), " --- |".repeat(12));
    assert!(text(cx).ends_with(&format!("| --- | --- |\n\n{wide}")), "{}", text(cx));
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
}
