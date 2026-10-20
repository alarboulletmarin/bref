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

    // A diagram written by a newer Bref opens in the viewer, with a notice: whatever is typed,
    // its bytes do not change.
    let future = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 60\"><rect width=\"100\" height=\"60\"/><metadata id=\"bref-diagram\">\nversion 99\nshape 1 hexagon 0 0 100 60 0 0 0 A\n</metadata></svg>\n";
    let newer = j.root.join("Futur.svg");
    fs::write(&newer, future).unwrap();
    // The editor must not have the focus when the window is drawn, or the file is put away.
    shell.update_in(cx, |s, window, cx| {
        s.toasts.clear();
        s.load_note(&newer, cx);
        let viewer = s.pane().viewer().cloned().unwrap();
        window.focus(&viewer.focus_handle(cx));
    });
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.pane().drawing().is_none() && s.pane().viewer().is_some() && s.told(Tone::Info, "Bref plus récent")));
    cx.simulate_keystrokes("+ - 0 delete secondary-z secondary-a backspace");
    assert_eq!(fs::read_to_string(&newer).unwrap(), future);
    // One of this version keeps the lines it cannot read, and does not reuse their ids.
    let mixed = j.root.join("Mixte.svg");
    let unknown = "shape 9 hexagon 0 0 10 10 0 0 0 H";
    fs::write(&mixed, format!("<svg><metadata id=\"bref-diagram\">\nversion 1\n{unknown}\nshape 1 rect 0 0 100 60 0 0 0 \n</metadata></svg>\n")).unwrap();
    let canvas = shell.update_in(cx, |s, window, cx| {
        s.toasts.clear();
        s.load_note(&mixed, cx);
        let canvas = s.pane().drawing().cloned().unwrap();
        window.focus(&canvas.focus_handle(cx));
        canvas
    });
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.toasts.is_empty()));
    cx.simulate_input("r");
    let (from, to) = canvas.read_with(cx, |c, _| (c.spot(200., 0.), c.spot(300., 80.)));
    cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    let written = fs::read_to_string(&mixed).unwrap();
    let edited = Diagram::from_svg(&written).unwrap();
    assert!(written.lines().any(|l| l == unknown) && edited.shapes.len() == 2 && edited.shapes[1].id == 10);
    cx.simulate_keystrokes("escape escape");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.pane().file.is_none()));
    fs::remove_file(&newer).unwrap();
    fs::remove_file(&mixed).unwrap();

    // Styled text: the marks of notes. A new diagram reads them; one made before them keeps its
    // stars until it is asked, and what is typed is shown as typed.
    cx.simulate_keystrokes("secondary-shift-d");
    cx.run_until_parked();
    let (styled, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
    let rows = |cx: &mut gpui::VisualTestContext, canvas: &Entity<Canvas>, from: (f32, f32), to: (f32, f32)| {
        let (from, to) = canvas.read_with(cx, |c, _| (c.spot(from.0, from.1), c.spot(to.0, to.1)));
        cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    };
    cx.simulate_input("r");
    rows(cx, &canvas, (0., 0.), (140., 60.));
    cx.simulate_keystrokes("enter");
    cx.simulate_input("*abstract()* <u>count</u> a * b");
    let saved = |file: &Path| fs::read_to_string(file).unwrap();
    // While it is written, the marks are text: the file has them, the drawing has not drawn them yet.
    assert!(saved(&styled).contains("*abstract()* &lt;u&gt;count&lt;/u&gt; a * b\n") && saved(&styled).contains("rich 1"));
    cx.simulate_keystrokes("escape");
    let svg = saved(&styled);
    assert!(svg.contains("<tspan font-style=\"italic\">abstract()</tspan>") && svg.contains("<tspan text-decoration=\"underline\">count</tspan>") && svg.contains("> a * b</tspan>"), "{svg}");
    assert!(!svg.contains(">*abstract()*"));
    // A diagram of a note's age: its stars stay, until the bar's button is used (and undone).
    let old = j.root.join("Ancien.svg");
    fs::write(&old, "<svg><metadata id=\"bref-diagram\">\nversion 1\nshape 1 rect 0 0 200 60 0 0 0 a *b* c\n</metadata></svg>\n").unwrap();
    let canvas = shell.update_in(cx, |s, window, cx| {
        s.load_note(&old, cx);
        let canvas = s.pane().drawing().cloned().unwrap();
        window.focus(&canvas.focus_handle(cx));
        canvas
    });
    cx.run_until_parked();
    cx.simulate_input("o");
    rows(cx, &canvas, (300., 0.), (400., 60.));
    assert!(!canvas.read_with(cx, |c, _| c.diagram().rich));
    let plain = saved(&old);
    assert!(plain.contains("version 1\n") && !plain.contains("rich") && plain.contains(">a *b* c</text>"));
    canvas.update(cx, |c, cx| c.enrich(cx));
    let rich = saved(&old);
    assert!(rich.contains("version 2\nrich 1\n") && rich.contains("<tspan font-style=\"italic\">b</tspan>"), "{rich}");
    cx.simulate_keystrokes("secondary-z");
    assert!(!canvas.read_with(cx, |c, _| c.diagram().rich) && saved(&old).contains("version 1\n"));
    cx.simulate_keystrokes("escape escape");
    cx.run_until_parked();
    fs::remove_file(&old).unwrap();
    // The texts at the ends of an arrow: Tab goes from one to the next, one step of undo for the
    // three; a text can be dragged along the stroke.
    cx.simulate_keystrokes("secondary-shift-d");
    cx.run_until_parked();
    let (ends, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
    cx.simulate_input("r");
    rows(cx, &canvas, (0., 0.), (140., 60.));
    cx.simulate_input("r");
    rows(cx, &canvas, (300., 0.), (440., 60.));
    cx.simulate_input("a");
    rows(cx, &canvas, (140., 30.), (300., 30.));
    let drawn = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().clone());
    let link = drawn(cx).links[0].clone();
    assert_eq!((link.from, link.to), (diagram::End::Pin(1, 1., 0.5), diagram::End::Pin(2, 0., 0.5)));
    cx.simulate_keystrokes("tab");
    cx.simulate_input("1");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("owns");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("0..*");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("é");
    cx.simulate_keystrokes("tab tab escape");
    let link = drawn(cx).links[0].clone();
    assert_eq!((link.start_text.as_str(), link.text.as_str(), link.end_text.as_str()), ("1", "owns", "0..*\né"));
    let svg = saved(&ends);
    assert!(svg.contains("version 3\nrich 1\n") && svg.contains("\nends 3 1\t0..*\\né\t0\t0\n"), "{svg}");
    // Drawn at the right spots: the start text beyond the arrow's start, the end text before its head.
    assert!(svg.contains("<text x=\"148\" y=\"24\" fill=\"#7d8590\" stroke=\"none\">1</text>"), "{svg}");
    assert!(svg.contains("text-anchor=\"end\"") && svg.contains(">0..*</text>") && svg.contains(">é</text>"), "{svg}");
    // One step of undo for the three texts, and one to put them back.
    cx.simulate_keystrokes("secondary-z");
    let link = drawn(cx).links[0].clone();
    assert!(!link.has_end_texts() && link.text.is_empty() && saved(&ends).contains("version 2\n"));
    cx.simulate_keystrokes("secondary-shift-z");
    assert_eq!(drawn(cx).links[0].end_text, "0..*\né");
    // Dragged along the stroke: the pointer goes toward the middle, the text follows.
    let before = drawn(cx);
    let trace = before.trace(&before.links[0]).unwrap();
    let (x, y, anchor) = diagram::Diagram::tip_label_at(&trace.tips[1], 0., 0, 2);
    assert_eq!(anchor, diagram::Anchor::End);
    let on = (x - 8., y + 6.);
    let click = canvas.read_with(cx, |c, _| c.spot(220., 30.));
    cx.simulate_click(click, gpui::Modifiers::none());
    rows(cx, &canvas, on, (on.0 - 50., on.1));
    let moved = drawn(cx).links[0].clone();
    assert!(moved.end_along > 40. && moved.start_along == 0., "{moved:?}");
    assert!(saved(&ends).contains(&format!("\t{}\n", moved.end_along)));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(drawn(cx).links[0].end_along, 0.);
    // A double click on the text writes in it; emptied, the arrow has no text at that end.
    let at = canvas.read_with(cx, |c, _| c.spot(on.0, on.1));
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.simulate_event(gpui::MouseDownEvent { position: at, modifiers: gpui::Modifiers::none(), button: MouseButton::Left, click_count: 2, first_mouse: false });
    cx.simulate_event(gpui::MouseUpEvent { position: at, modifiers: gpui::Modifiers::none(), button: MouseButton::Left, click_count: 2 });
    cx.simulate_keystrokes("backspace backspace backspace backspace backspace backspace escape");
    assert_eq!(drawn(cx).links[0].end_text, "");
    cx.simulate_keystrokes("escape escape");
    cx.run_until_parked();
    fs::remove_file(&ends).unwrap();
    // A class: typed as UML notation, it grows to hold its lines and is drawn with its styles.
    cx.simulate_keystrokes("secondary-shift-d");
    cx.run_until_parked();
    let (class, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
    cx.simulate_input("r");
    rows(cx, &canvas, (0., 0.), (60., 40.));
    cx.simulate_keystrokes("enter");
    let long = "- veryLongMemberNameWithoutAnySpaceToMakeTheBoxGrow(argument: List<String>): Map<String, List<Integer>>";
    for line in ["«interface»", "Shape", "---", "+ static count: int", "---", "+ abstract area(): f32", long] {
        cx.simulate_input(line);
        cx.simulate_keystrokes("enter");
    }
    cx.simulate_keystrokes("backspace escape");
    let shape = canvas.read_with(cx, |c, _| c.diagram().shapes[0].clone());
    assert!(shape.w >= 600. && shape.h >= 130. && shape.text.ends_with(long), "{shape:?}");
    let svg = saved(&class);
    assert!(svg.contains("font-weight=\"bold\" fill") && svg.contains("text-decoration=\"underline\" fill=\"#7d8590\" stroke=\"none\">+ count: int</text>") && svg.contains("font-style=\"italic\" fill=\"#7d8590\" stroke=\"none\">+ area(): f32</text>"), "{svg}");
    // Typing a text with a lone `---` and nothing else makes no class of it: it is drawn as typed.
    cx.simulate_keystrokes("escape escape");
    cx.run_until_parked();
    fs::remove_file(&class).unwrap();
    // Relationships: `k` opens the menu, a letter picks one. Each of the seventeen is drawn with its look,
    // and recognised again from the file.
    {
        use crate::logic::relation::Relation;
        cx.simulate_keystrokes("secondary-shift-d");
        cx.run_until_parked();
        let (rel, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
        let tool = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.tool());
        cx.simulate_input("r");
        rows(cx, &canvas, (0., 0.), (140., 400.));
        cx.simulate_input("r");
        rows(cx, &canvas, (300., 0.), (440., 400.));
        for (i, relation) in Relation::ALL.into_iter().enumerate() {
            if relation == Relation::AssociationClass {
                continue;
            }
            cx.simulate_input("k");
            cx.simulate_input(&relation.key().to_string());
            assert_eq!(tool(cx), canvas::Tool::Link(relation), "{relation:?}");
            let y = 20. + 22. * i as f32;
            rows(cx, &canvas, (150., y), (290., y));
            let link = canvas.read_with(cx, |c, _| c.diagram().links.last().cloned().unwrap());
            assert_eq!(Relation::of(&link), Some(relation), "{relation:?}: {link:?}");
            assert_eq!(link.text, relation.stereotype().map_or(String::new(), |n| format!("«{n}»")), "{relation:?}");
            // One arrow per pick: the tool goes back to the selection. (Escape drops the arrow's selection,
            // else the next pick would change it instead of drawing.)
            assert_eq!(tool(cx), canvas::Tool::Select);
            cx.simulate_keystrokes("escape");
        }
        let svg = saved(&rel);
        assert!(svg.contains("version 4\nrich 1\n") && svg.contains(" ball ") && svg.contains(" socket ") && svg.contains(" cross "), "{svg}");
        // Escape closes the menu: the next letter is a tool, not a relationship.
        cx.simulate_input("k");
        cx.simulate_keystrokes("escape");
        cx.simulate_input("a");
        assert_eq!(tool(cx), canvas::Tool::Link(Relation::Navigable));
        // A letter that is nowhere in the menu closes it, then acts as usual.
        cx.simulate_input("k");
        cx.simulate_input("o");
        assert_eq!(tool(cx), canvas::Tool::Shape(diagram::Form::Ellipse));
        cx.simulate_input("k");
        cx.simulate_input("7");
        assert_eq!(tool(cx), canvas::Tool::Shape(diagram::Form::Ellipse));
        cx.simulate_keystrokes("escape");
        // The n-ary association: a small diamond.
        cx.simulate_input("k");
        cx.simulate_input("y");
        let at = canvas.read_with(cx, |c, _| c.spot(520., 100.));
        cx.simulate_click(at, gpui::Modifiers::none());
        let diamond = canvas.read_with(cx, |c, _| c.diagram().shapes.last().cloned().unwrap());
        assert_eq!((diamond.form, diamond.w, diamond.h), (diagram::Form::Branch, 40., 40.));
        cx.simulate_keystrokes("escape escape");
        cx.run_until_parked();
        fs::remove_file(&rel).unwrap();
    }
    // An association class: a dotted line from a class to the middle of an association, which follows it.
    {
        use crate::logic::relation::Relation;
        cx.simulate_keystrokes("secondary-shift-d");
        cx.run_until_parked();
        let (ac, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
        for (from, to) in [((0., 0.), (100., 60.)), ((300., 0.), (400., 60.)), ((150., 200.), (250., 260.))] {
            cx.simulate_input("r");
            rows(cx, &canvas, from, to);
        }
        cx.simulate_input("l");
        rows(cx, &canvas, (90., 30.), (310., 30.));
        let assoc = canvas.read_with(cx, |c, _| c.diagram().links[0].id);
        cx.simulate_keystrokes("escape");
        cx.simulate_input("k");
        cx.simulate_input("k");
        rows(cx, &canvas, (200., 230.), (203., 36.));
        let d = canvas.read_with(cx, |c, _| c.diagram().clone());
        assert_eq!(d.links[1].to, diagram::End::Mid(assoc));
        assert_eq!(Relation::of(&d.links[1]), Some(Relation::AssociationClass));
        assert_eq!(d.ends(&d.links[1]).unwrap().1, (200., 30.));
        assert!(saved(&ac).contains(&format!(" m{assoc} ")) && saved(&ac).contains("version 4\n"));
        // The association moves with its end shape: the dotted line still meets its middle.
        rows(cx, &canvas, (350., 30.), (350., 130.));
        let d = canvas.read_with(cx, |c, _| c.diagram().clone());
        assert_eq!(d.ends(&d.links[1]).unwrap().1, d.mid(assoc).unwrap());
        // Removing the association takes the dotted line along, in one step of undo.
        let click = canvas.read_with(cx, |c, _| c.spot(130., 30.));
        cx.simulate_click(click, gpui::Modifiers::none());
        cx.simulate_keystrokes("delete");
        assert_eq!(canvas.read_with(cx, |c, _| c.diagram().links.len()), 0);
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(canvas.read_with(cx, |c, _| c.diagram().links.len()), 2);
        // With the association selected, the menu changes it instead of drawing another.
        let click = canvas.read_with(cx, |c, _| c.spot(130., 30.));
        cx.simulate_click(click, gpui::Modifiers::none());
        cx.simulate_input("k");
        cx.simulate_input("h");
        let first = canvas.read_with(cx, |c, _| c.diagram().links[0].clone());
        assert_eq!(Relation::of(&first), Some(Relation::Generalization));
        cx.simulate_keystrokes("escape escape");
        cx.run_until_parked();
        fs::remove_file(&ac).unwrap();
    }
    // Containers: a package of five classes moves as one, a class dragged out leaves it, a class dragged in
    // joins it, and one step of undo puts both the place and the holder back.
    {
        cx.simulate_keystrokes("secondary-shift-d");
        cx.run_until_parked();
        let (hold, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
        let drawn = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().clone());
        cx.simulate_input("g");
        rows(cx, &canvas, (0., 0.), (400., 300.));
        for i in 0..5 {
            cx.simulate_input("r");
            let (x, y) = (20. + 120. * (i % 3) as f32, 60. + 90. * (i / 3) as f32);
            rows(cx, &canvas, (x, y), (x + 100., y + 60.));
        }
        let d = drawn(cx);
        let pack = d.shapes[0].id;
        assert_eq!((d.shapes[0].form, d.shapes.len()), (diagram::Form::Package, 6));
        assert!(d.shapes[1..].iter().all(|s| s.parent == Some(pack)), "created over the package, held by it");
        let svg = saved(&hold);
        assert!(svg.contains("version 5\n") && svg.matches("\nin ").count() == 5, "{svg}");
        // Dragged by an empty part of the package, it takes its classes along.
        rows(cx, &canvas, (350., 250.), (450., 350.));
        let moved = drawn(cx);
        assert_eq!((moved.shapes[0].x, moved.shapes[0].y), (100., 100.));
        assert!(moved.shapes.iter().zip(&d.shapes).skip(1).all(|(m, o)| (m.x, m.y) == (o.x + 100., o.y + 100.)));
        // One class dragged out of the package: it leaves it, the others do not.
        let out = moved.shapes[1].clone();
        rows(cx, &canvas, (out.x + 50., out.y + 30.), (-150., 50.));
        let after = drawn(cx);
        assert_eq!(after.shapes.iter().filter(|s| s.parent == Some(pack)).count(), 4);
        assert!(after.shape(out.id).unwrap().parent.is_none() && after.shape(out.id).unwrap().x < 0.);
        // One step of undo: back in place, and back in the package.
        cx.simulate_keystrokes("secondary-z");
        let undone = drawn(cx);
        assert_eq!(undone.shape(out.id), moved.shape(out.id));
        assert_eq!(undone.shapes.iter().filter(|s| s.parent == Some(pack)).count(), 5);
        // Dragged out and back in, it is held again.
        rows(cx, &canvas, (out.x + 50., out.y + 30.), (-150., 50.));
        rows(cx, &canvas, (-150., 50.), (out.x + 50., out.y + 30.));
        assert_eq!(drawn(cx).shape(out.id).unwrap().parent, Some(pack));
        // Fit to content: the package closes around its classes.
        let click = canvas.read_with(cx, |c, _| c.spot(450., 380.));
        cx.simulate_click(click, gpui::Modifiers::none());
        assert_eq!(canvas.read_with(cx, |c, _| c.picked()), 1);
        canvas.update(cx, |c, cx| c.fit_container(cx));
        let fit = drawn(cx).shapes[0].clone();
        assert!(fit.w < 400. && fit.h < 300. && fit.x <= 120. && fit.y <= 160., "{fit:?}");
        // Copied, it copies what it holds; removed, it removes it.
        cx.simulate_keystrokes("secondary-d");
        assert_eq!(drawn(cx).shapes.len(), 12);
        cx.simulate_keystrokes("delete");
        assert_eq!(drawn(cx).shapes.len(), 6);
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(drawn(cx).shapes.len(), 12);
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(drawn(cx).shapes.len(), 6);
        // Everything selected and removed: the package, its classes, nothing left behind.
        cx.simulate_keystrokes("secondary-a delete");
        assert_eq!(drawn(cx).shapes.len(), 0);
        cx.simulate_keystrokes("escape escape");
        cx.run_until_parked();
        fs::remove_file(&hold).unwrap();
    }
    // A package diagram and an object diagram, made of what the engine already has: containers, notation, relationships.
    {
        use crate::logic::relation::Relation;
        cx.simulate_keystrokes("secondary-shift-d");
        cx.run_until_parked();
        let (pk, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
        let drawn = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().clone());
        let write = |cx: &mut gpui::VisualTestContext, lines: &[&str]| {
            cx.simulate_keystrokes("enter");
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    cx.simulate_keystrokes("enter");
                }
                cx.simulate_input(line);
            }
            cx.simulate_keystrokes("escape");
        };
        // Two packages with their qualified name under the tab, a third inside the first, an import between them.
        cx.simulate_input("g");
        rows(cx, &canvas, (0., 0.), (300., 200.));
        write(cx, &["billing", "pkg::billing"]);
        cx.simulate_input("g");
        rows(cx, &canvas, (400., 0.), (700., 200.));
        write(cx, &["shipping"]);
        cx.simulate_input("g");
        rows(cx, &canvas, (20., 80.), (140., 170.));
        write(cx, &["invoices"]);
        cx.simulate_keystrokes("escape");
        cx.simulate_input("k");
        cx.simulate_input("i");
        rows(cx, &canvas, (295., 100.), (405., 100.));
        let d = drawn(cx);
        let (billing, shipping, invoices) = (d.shapes[0].clone(), d.shapes[1].clone(), d.shapes[2].clone());
        assert_eq!((invoices.parent, shipping.parent), (Some(billing.id), None));
        let import = d.links[0].clone();
        assert_eq!((Relation::of(&import), import.dashed, import.text.as_str()), (Some(Relation::Import), true, "«import»"));
        assert_eq!((import.from, import.to), (diagram::End::Pin(billing.id, 1., 0.5), diagram::End::Pin(shipping.id, 0., 0.5)));
        let svg = saved(&pk);
        assert!(svg.contains(">billing</text>") && svg.contains(">pkg::billing</text>") && svg.contains(">invoices</text>") && svg.contains("version 5\n"), "{svg}");
        // An object diagram: instances are underlined, their slots are not; one has no name; they are linked.
        cx.simulate_keystrokes("escape");
        cx.simulate_input("r");
        rows(cx, &canvas, (20., 300.), (220., 380.));
        write(cx, &["order1 : Order", "---", "total = 12"]);
        cx.simulate_input("r");
        rows(cx, &canvas, (400., 300.), (600., 380.));
        write(cx, &[": Customer", "---", "name = \"Ann\""]);
        cx.simulate_keystrokes("escape");
        cx.simulate_input("l");
        rows(cx, &canvas, (215., 340.), (405., 340.));
        cx.simulate_keystrokes("enter");
        cx.simulate_input("placed by");
        cx.simulate_keystrokes("escape");
        let svg = saved(&pk);
        assert!(svg.contains("text-decoration=\"underline\" fill=\"#7d8590\" stroke=\"none\">order1 : Order</text>"), "{svg}");
        assert!(svg.contains(">: Customer</text>") && svg.contains(">total = 12</text>") && svg.contains(">name = \"Ann\"</text>") && svg.contains(">placed by</text>"), "{svg}");
        let underlined = svg.matches("text-decoration=\"underline\"").count();
        assert_eq!(underlined, 2, "only the two instances: {svg}");
        cx.simulate_keystrokes("escape escape");
        cx.run_until_parked();
        fs::remove_file(&pk).unwrap();
    }
    // A use case diagram: a system boundary that holds use cases, actors outside, and the relationships between them.
    {
        use crate::logic::relation::Relation;
        cx.simulate_keystrokes("secondary-shift-d");
        cx.run_until_parked();
        let (uc, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
        let drawn = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().clone());
        let write = |cx: &mut gpui::VisualTestContext, lines: &[&str]| {
            cx.simulate_keystrokes("enter");
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    cx.simulate_keystrokes("enter");
                }
                cx.simulate_input(line);
            }
            cx.simulate_keystrokes("escape");
        };
        cx.simulate_input("f");
        rows(cx, &canvas, (100., 0.), (500., 300.));
        write(cx, &["Shop"]);
        for (from, to, text) in [((150., 60.), (310., 140.), vec!["Pay", "---", "extension points", "by card"]), ((150., 180.), (310., 260.), vec!["Invoice"]), ((340., 60.), (480., 140.), vec!["Discount"])] {
            cx.simulate_input("o");
            rows(cx, &canvas, from, to);
            write(cx, &text);
        }
        for (from, to, text) in [((0., 100.), (40., 180.), "Customer"), ((0., 220.), (40., 300.), "Member")] {
            cx.simulate_input("p");
            rows(cx, &canvas, from, to);
            write(cx, &[text]);
        }
        cx.simulate_keystrokes("escape");
        let links = [("line", (20., 140.), (230., 100.)), ("l", (230., 100.), (230., 220.)), ("e", (410., 100.), (230., 100.)), ("h", (20., 260.), (20., 140.))];
        for (letter, from, to) in links {
            if letter == "line" {
                cx.simulate_input("l");
            } else {
                cx.simulate_input("k");
                cx.simulate_input(letter);
            }
            rows(cx, &canvas, from, to);
            cx.simulate_keystrokes("escape");
        }
        let d = drawn(cx);
        let frame = d.shapes[0].id;
        let held: Vec<bool> = d.shapes.iter().skip(1).map(|s| s.parent == Some(frame)).collect();
        assert_eq!(held, [true, true, true, false, false], "use cases are in the boundary, actors are not");
        let kinds: Vec<Option<Relation>> = d.links.iter().map(Relation::of).collect();
        assert_eq!(kinds, [Some(Relation::Association), Some(Relation::Include), Some(Relation::Extend), Some(Relation::Generalization)]);
        let svg = saved(&uc);
        // The extension points are under the name, behind a separator that stays inside the ellipse.
        let pay = &d.shapes[1];
        assert_eq!(pay.text, "Pay\n---\nextension points\nby card");
        let rule = svg.lines().find(|l| l.starts_with("<path d=\"M") && l.contains('H') && !l.contains('L') && !l.contains('C')).unwrap();
        let (x0, x1) = rule.trim_start_matches("<path d=\"M").split_once("\"").unwrap().0.split_once(' ').map(|(x, rest)| (x.parse::<f32>().unwrap(), rest.split_once('H').unwrap().1.parse::<f32>().unwrap())).unwrap();
        assert!(x0 > pay.x && x1 < pay.x + pay.w, "{rule}");
        assert!(svg.contains(">«include»</text>") && svg.contains(">«extend»</text>") && svg.contains(">Customer</text>") && svg.contains(">Shop</text>"), "{svg}");
        cx.simulate_keystrokes("escape escape");
        cx.run_until_parked();
        fs::remove_file(&uc).unwrap();
    }
    // The library: `s` opens it, typing searches it, Enter picks the first shape, a click draws it at its size
    // with its text, a drop places it, and a starting point opens a new diagram that already holds a few shapes.
    {
        use crate::logic::library;
        cx.simulate_keystrokes("secondary-shift-d");
        cx.run_until_parked();
        let (lib, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
        let query = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.library_query().map(str::to_string));
        let tool = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.tool());
        assert_eq!(query(cx), None);
        cx.simulate_input("s");
        assert_eq!(query(cx), Some(String::new()));
        // What is typed is the search, not the tools' letters (`a` is the arrow, `r` the rectangle).
        cx.simulate_input("ar");
        assert_eq!((query(cx), tool(cx)), (Some("ar".into()), canvas::Tool::Select));
        cx.simulate_keystrokes("backspace backspace backspace");
        let interface = library::presets().iter().position(|p| p.matches(tr("interface", "interface"))).unwrap();
        cx.simulate_input(tr("interface", "interface"));
        // Escape closes; `s` opens again, empty.
        cx.simulate_keystrokes("escape");
        assert_eq!(query(cx), None);
        cx.simulate_input("s");
        assert_eq!(query(cx), Some(String::new()));
        cx.simulate_input(tr("interface", "interface"));
        cx.simulate_keystrokes("enter");
        assert_eq!((query(cx), tool(cx)), (None, canvas::Tool::Preset(interface)));
        // A click draws it, at the size and with the text of the preset.
        let at = canvas.read_with(cx, |c, _| c.spot(100., 100.));
        cx.simulate_click(at, gpui::Modifiers::none());
        let d = canvas.read_with(cx, |c, _| c.diagram().clone());
        let preset = &library::presets()[interface];
        assert_eq!((d.shapes[0].w, d.shapes[0].h, d.shapes[0].text.as_str()), (preset.w, preset.h, preset.text));
        assert!(saved(&lib).contains("«interface»"), "{}", saved(&lib));
        // Dragged out with the mouse, it is the size dragged.
        cx.simulate_input("s");
        cx.simulate_input(tr("package", "paquetage"));
        cx.simulate_keystrokes("enter");
        rows(cx, &canvas, (300., 0.), (500., 100.));
        let d = canvas.read_with(cx, |c, _| c.diagram().clone());
        assert_eq!((d.shapes[1].form, d.shapes[1].w, d.shapes[1].h), (diagram::Form::Package, 200., 100.));
        // A drop places it centered on the pointer, inside a container if it is over one; no shape answers a strange search.
        let at = canvas.read_with(cx, |c, _| c.spot(400., 60.));
        let actor = library::presets().iter().position(|p| p.form == diagram::Form::Actor).unwrap();
        canvas.update(cx, |c, cx| c.drop_preset(actor, at, cx));
        let d = canvas.read_with(cx, |c, _| c.diagram().clone());
        assert_eq!((d.shapes[2].form, d.shapes[2].parent, d.shapes[2].x), (diagram::Form::Actor, Some(d.shapes[1].id), 380.));
        cx.simulate_input("s");
        cx.simulate_input("|\\ é 日本語 😀");
        assert_eq!(query(cx).as_deref(), Some("|\\ é 日本語 😀"));
        cx.simulate_keystrokes("enter");
        assert_eq!(tool(cx), canvas::Tool::Select, "nothing answers: Enter picks nothing");
        // A starting point opens a new diagram of its own, and leaves this one as it is.
        let before = saved(&lib);
        canvas.update(cx, |c, cx| c.start_template(0, cx));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let (new, fresh) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
        assert_ne!(new, lib);
        assert_eq!(saved(&lib), before);
        let d = fresh.read_with(cx, |c, _| c.diagram().clone());
        assert_eq!((d.shapes.len(), d.links.len(), d.rich), (2, 1, true));
        assert_eq!(saved(&new), d.to_svg(diagram::COLORS[0]));
        cx.simulate_keystrokes("escape escape");
        cx.run_until_parked();
        fs::remove_file(&lib).unwrap();
        fs::remove_file(&new).unwrap();
    }
    fs::remove_file(&styled).unwrap();
    j.settle(cx);
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
}
