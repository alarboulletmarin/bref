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
    // The bar over a selected arrow can cover what is next to it: the selection is dropped first.
    cx.simulate_keystrokes("escape");
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
    // Relationships: `k` opens the menu, a letter picks one. Each of them is drawn with its look,
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
            let y = 20. + 19. * i as f32;
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
        assert!(svg.contains("version 7\nrich 1\n") && svg.contains(" ball ") && svg.contains(" socket ") && svg.contains(" cross "), "{svg}");
        // Escape closes the menu: the next letter is a tool, not a relationship.
        cx.simulate_input("k");
        cx.simulate_keystrokes("escape");
        cx.simulate_input("a");
        assert_eq!(tool(cx), canvas::Tool::Link(Relation::Navigable));
        // The association class has the letter `R` in the menu, not the rectangle's meaning.
        cx.simulate_input("k");
        cx.simulate_input("r");
        assert_eq!(tool(cx), canvas::Tool::Link(Relation::AssociationClass));
        // A key that is nowhere in the menu closes it, then acts as usual.
        cx.simulate_keystrokes("escape");
        cx.simulate_input("r");
        cx.simulate_input("k");
        cx.simulate_input("7");
        assert_eq!(tool(cx), canvas::Tool::Shape(diagram::Form::Rect));
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
        cx.simulate_input("r");
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
    // An elbowed arrow goes around the shape in its way, and follows when the shape moves.
    {
        cx.simulate_keystrokes("secondary-shift-d");
        cx.run_until_parked();
        let (way, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
        let drawn = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().clone());
        for (from, to) in [((0., 100.), (100., 160.)), ((400., 100.), (500., 160.)), ((190., 0.), (250., 300.))] {
            cx.simulate_input("r");
            rows(cx, &canvas, from, to);
        }
        cx.simulate_input("a");
        rows(cx, &canvas, (90., 130.), (410., 130.));
        let d = drawn(cx);
        let wall = d.shapes[2].clone();
        let through = |d: &diagram::Diagram| {
            let path = d.trace(&d.links[0]).unwrap().points;
            path.windows(2).any(|w| {
                let (x0, x1, y0, y1) = (w[0].0.min(w[1].0), w[0].0.max(w[1].0), w[0].1.min(w[1].1), w[0].1.max(w[1].1));
                x0 < wall.x + wall.w && x1 > wall.x && y0 < wall.y + wall.h && y1 > wall.y
            })
        };
        assert!(!through(&d) && d.trace(&d.links[0]).unwrap().points.len() >= 5, "{:?}", d.trace(&d.links[0]).unwrap().points);
        // Take the wall away (drag it far below): the arrow is straight again.
        cx.simulate_keystrokes("escape");
        rows(cx, &canvas, (220., 150.), (220., 20.));
        let up = drawn(cx);
        assert!(up.shapes[2].y < wall.y);
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(drawn(cx).shapes[2].y, wall.y);
        cx.simulate_keystrokes("escape escape");
        cx.run_until_parked();
        fs::remove_file(&way).unwrap();
    }
    // A state machine: states and pseudostates from the library, transitions with their label, one that loops.
    {
        use crate::logic::library;
        cx.simulate_keystrokes("secondary-shift-d");
        cx.run_until_parked();
        let (sm, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
        let drawn = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().clone());
        let presets = library::presets();
        let pick = |form: diagram::Form, name: &str| presets.iter().position(|p| p.form == form && (name.is_empty() || p.name == name)).unwrap();
        let (initial, state, last) = (pick(diagram::Form::Dot, tr("Initial state", "État initial")), pick(diagram::Form::Round, ""), pick(diagram::Form::Final, ""));
        for (preset, at) in [(initial, (20., 100.)), (state, (200., 100.)), (last, (460., 100.))] {
            let at = canvas.read_with(cx, |c, _| c.spot(at.0, at.1));
            canvas.update(cx, |c, cx| c.drop_preset(preset, at, cx));
        }
        cx.simulate_keystrokes("escape");
        let d = drawn(cx);
        assert_eq!(d.shapes.iter().map(|s| s.form).collect::<Vec<_>>(), [diagram::Form::Dot, diagram::Form::Round, diagram::Form::Final]);
        let at = |d: &diagram::Diagram, i: usize| d.shapes[i].center();
        for (from, to, label) in [(0, 1, ""), (1, 2, "stop [ready] / log()")] {
            cx.simulate_input("a");
            let ((x0, y0), (x1, y1)) = (at(&d, from), at(&d, to));
            rows(cx, &canvas, (x0, y0), (x1, y1));
            if !label.is_empty() {
                cx.simulate_keystrokes("enter");
                cx.simulate_input(label);
                cx.simulate_keystrokes("escape");
            }
            cx.simulate_keystrokes("escape");
        }
        // The state selected: a loop on it, its label written at once.
        let click = canvas.read_with(cx, |c, _| c.spot(at(&d, 1).0 - 40., at(&d, 1).1 + 20.));
        cx.simulate_click(click, gpui::Modifiers::none());
        canvas.update(cx, |c, cx| c.self_link(cx));
        cx.simulate_input("tick / n++");
        cx.simulate_keystrokes("escape");
        let d = drawn(cx);
        let own = d.links.last().unwrap();
        assert_eq!((own.from, own.to, own.text.as_str()), (diagram::End::Shape(d.shapes[1].id), diagram::End::Shape(d.shapes[1].id), "tick / n++"));
        let svg = saved(&sm);
        assert!(svg.contains("version 6\n") && svg.contains("<tspan font-style=\"italic\">[ready]</tspan>") && svg.contains(">tick / n++</text>"), "{svg}");
        assert!(svg.contains("fill=\"#7d8590\" stroke") || svg.matches(" fill=\"#").count() >= 2, "the dot is filled: {svg}");
        // One step of undo takes the loop away; the label is not left behind.
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(drawn(cx).links.len(), 2);
        cx.simulate_keystrokes("escape escape");
        cx.run_until_parked();
        fs::remove_file(&sm).unwrap();
        // The starting point: a traffic light, with its loop.
        let (from, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned(), s.pane().drawing().cloned()));
        assert!(from.is_none() && canvas.is_none());
    }
    // A sequence diagram: lifelines from the library, messages drawn between them, placed by their order; one dragged up
    // goes first, in one step of undo; a message to a lifeline itself loops; numbering is shown, not typed.
    {
        use crate::logic::{library, relation::Relation};
        cx.simulate_keystrokes("secondary-shift-d");
        cx.run_until_parked();
        let (sq, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
        let drawn = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().clone());
        let life = library::presets().iter().position(|p| p.form == diagram::Form::Lifeline).unwrap();
        for x in [100., 350., 600.] {
            let at = canvas.read_with(cx, |c, _| c.spot(x, 150.));
            canvas.update(cx, |c, cx| c.drop_preset(life, at, cx));
        }
        cx.simulate_keystrokes("escape");
        let d = drawn(cx);
        assert_eq!(d.shapes.len(), 3);
        let lives: Vec<u32> = d.shapes.iter().map(|s| s.id).collect();
        let send = |cx: &mut gpui::VisualTestContext, keys: &str, from: f32, to: f32, text: &str| {
            cx.simulate_input("k");
            cx.simulate_input(keys);
            rows(cx, &canvas, (from, 100.), (to, 100.));
            if !text.is_empty() {
                cx.simulate_keystrokes("enter");
                cx.simulate_input(text);
                cx.simulate_keystrokes("escape");
            }
            cx.simulate_keystrokes("escape");
        };
        send(cx, "f", 100., 350., "login()");
        send(cx, "f", 350., 600., "find()");
        send(cx, "d", 600., 350., "user");
        send(cx, "d", 350., 100., "token");
        let d = drawn(cx);
        // They are between the lifelines, at 44 from each other from the first, under the heads (10 + 36).
        let ys: Vec<f32> = d.messages().iter().map(|id| d.message_y(*id).unwrap()).collect();
        assert_eq!(ys, [82., 126., 170., 214.]);
        assert!(d.links.iter().all(|l| matches!((l.from, l.to), (diagram::End::Life(_), diagram::End::Life(_)))), "{:?}", d.links.iter().map(|l| (l.from, l.to)).collect::<Vec<_>>());
        assert_eq!(d.links.iter().map(Relation::of).collect::<Vec<_>>(), [Some(Relation::Sync), Some(Relation::Sync), Some(Relation::Dependency), Some(Relation::Dependency)]);
        // The lifelines go to the last message and a little more.
        assert!(d.shapes.iter().all(|s| s.y == 10. && s.y + s.h >= 214. + 44.), "{:?}", d.shapes);
        // The third message, dragged up above the first, takes the first place; one step of undo puts it back.
        let third = d.messages()[2];
        let at = (475., ys[2]);
        rows(cx, &canvas, at, (475., ys[0] - 10.));
        let moved = drawn(cx);
        assert_eq!(moved.messages()[0], third);
        assert_eq!(moved.message_y(third), Some(82.));
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(drawn(cx).messages(), d.messages());
        // A message from a lifeline back to it is a loop on its right.
        send(cx, "f", 350., 350., "check()");
        let d = drawn(cx);
        let own = d.links.last().unwrap();
        assert_eq!((own.from, own.to), (diagram::End::Life(lives[1]), diagram::End::Life(lives[1])));
        let path = d.trace(own).unwrap().points;
        assert_eq!(path.len(), 4, "{path:?}");
        // Numbered, the messages show their place and their text is untouched; the file says so.
        let before = saved(&sq);
        assert!(before.contains("version 7\n") && !before.contains("numbered") && !before.contains(">1: login()"));
        canvas.update(cx, |c, cx| c.toggle_numbers(cx));
        let numbered = saved(&sq);
        assert!(numbered.contains("numbered 1\n") && numbered.contains(">1: login()</text>") && numbered.contains(">5: check()</text>"), "{numbered}");
        assert_eq!(drawn(cx).links[0].text, "login()");
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(saved(&sq), before);
        // Both lifelines of a message removed with their messages: nothing is left hanging.
        cx.simulate_keystrokes("secondary-a delete");
        assert_eq!((drawn(cx).shapes.len(), drawn(cx).links.len()), (0, 0));
        cx.simulate_keystrokes("escape escape");
        cx.run_until_parked();
        fs::remove_file(&sq).unwrap();
    }
    // Combined fragments: choosing one in the library with messages selected wraps them; a message inside is picked
    // through the fragment; let go inside or outside, it joins or leaves, in one step of undo; "else" is reached low.
    {
        use crate::logic::library;
        cx.simulate_keystrokes("secondary-shift-d");
        cx.run_until_parked();
        let (fr, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
        let drawn = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().clone());
        let life = library::presets().iter().position(|p| p.form == diagram::Form::Lifeline).unwrap();
        for x in [100., 350., 600.] {
            let at = canvas.read_with(cx, |c, _| c.spot(x, 150.));
            canvas.update(cx, |c, cx| c.drop_preset(life, at, cx));
        }
        cx.simulate_keystrokes("escape");
        for (from, to) in [(100., 350.), (350., 600.), (600., 350.), (350., 100.)] {
            cx.simulate_input("k");
            cx.simulate_input(if from < to { "f" } else { "d" });
            rows(cx, &canvas, (from, 100.), (to, 100.));
            cx.simulate_keystrokes("escape");
        }
        let plain = drawn(cx);
        let m = plain.messages();
        let ys0: Vec<f32> = m.iter().map(|id| plain.message_y(*id).unwrap()).collect();
        assert_eq!(ys0, [82., 126., 170., 214.]);
        let pick = |cx: &mut gpui::VisualTestContext, x: f32, y: f32, mods: gpui::Modifiers| {
            let at = canvas.read_with(cx, |c, _| c.spot(x, y));
            cx.simulate_click(at, mods);
        };
        let choose = |cx: &mut gpui::VisualTestContext, name: &str| {
            cx.simulate_input("s");
            cx.simulate_input(name);
            cx.simulate_keystrokes("enter");
        };
        // Nothing selected: the fragment is a tool, drawn by hand, and nothing is wrapped.
        choose(cx, "ref");
        assert_eq!(canvas.read_with(cx, |c, _| c.tool()), canvas::Tool::Preset(library::presets().iter().position(|p| p.text.starts_with("ref")).unwrap()));
        cx.simulate_keystrokes("escape");
        assert!(drawn(cx).shapes.iter().all(|s| s.form != diagram::Form::Fragment));
        // The second and third messages selected, "alt" wraps them: room above the first, the frame around both.
        pick(cx, 475., 126., gpui::Modifiers::none());
        pick(cx, 475., 170., gpui::Modifiers { shift: true, ..Default::default() });
        assert_eq!(canvas.read_with(cx, |c, _| c.picked()), 2);
        choose(cx, "alt");
        let d = drawn(cx);
        let frag = d.shapes.iter().find(|s| s.form == diagram::Form::Fragment).unwrap().clone();
        assert_eq!(frag.text, tr("alt\n[condition]\n[else]", "alt\n[condition]\n[sinon]"));
        assert_eq!(canvas.read_with(cx, |c, _| c.picked()), 1);
        let ys: Vec<f32> = m.iter().map(|id| d.message_y(*id).unwrap()).collect();
        assert_eq!(ys, [82., 172., 216., 274.]);
        assert_eq!((frag.y, frag.y + frag.h), (132., 242.));
        assert!(m[1..3].iter().all(|id| d.link(*id).unwrap().holder == Some(frag.id)) && d.link(m[0]).unwrap().holder.is_none() && d.link(m[3]).unwrap().holder.is_none());
        // The lifelines go under the foot of the fragment and the last message.
        assert!(d.shapes.iter().filter(|s| s.form == diagram::Form::Lifeline).all(|s| s.y + s.h >= 274. + 44.));
        // A click on a message inside the fragment picks the message, not the fragment.
        pick(cx, 475., 172., gpui::Modifiers::none());
        assert_eq!((canvas.read_with(cx, |c, _| c.picked()), canvas.read_with(cx, |c, _| c.selected_ids())), (1, vec![m[1]]));
        // The last message dragged up into the fragment joins it; one step of undo puts it back out.
        rows(cx, &canvas, (225., 274.), (225., 205.));
        let d = drawn(cx);
        assert_eq!(d.messages(), [m[0], m[1], m[3], m[2]]);
        assert_eq!(d.link(m[3]).unwrap().holder, Some(frag.id));
        cx.simulate_keystrokes("secondary-z");
        let d = drawn(cx);
        assert!(d.messages() == m && d.link(m[3]).unwrap().holder.is_none());
        // The first one dragged down between the two inside joins too; dragged down under everything, it leaves again.
        rows(cx, &canvas, (225., 82.), (225., 216.));
        let d = drawn(cx);
        assert_eq!(d.link(m[0]).unwrap().holder, Some(frag.id), "{:?}", d.messages());
        let at = d.message_y(m[0]).unwrap();
        rows(cx, &canvas, (225., at), (225., 330.));
        let d = drawn(cx);
        assert_eq!((d.messages().last().copied(), d.link(m[0]).unwrap().holder), (Some(m[0]), None));
        // The file carries it, and reads back the same.
        let text = saved(&fr);
        assert!(text.contains("version 8\n") && text.contains(&format!("held {} {} 0\n", m[1], frag.id)), "{text}");
        assert!(Diagram::from_svg(&text).unwrap() == d);
        // Removing the fragment lets its messages go; they stay, in order, in one step of undo.
        pick(cx, 290., frag.y + 10., gpui::Modifiers::none());
        assert_eq!(canvas.read_with(cx, |c, _| c.selected_ids()), vec![frag.id]);
        cx.simulate_keystrokes("delete");
        let gone = drawn(cx);
        assert!(gone.shapes.iter().all(|s| s.form != diagram::Form::Fragment) && gone.messages().len() == 4 && gone.links.iter().all(|l| l.holder.is_none()));
        cx.simulate_keystrokes("secondary-z");
        assert!(drawn(cx).shapes.iter().any(|s| s.form == diagram::Form::Fragment));
        cx.simulate_keystrokes("secondary-a delete escape escape");
        cx.run_until_parked();
        fs::remove_file(&fr).unwrap();
    }
    // Activity diagrams: a row of the library holds an action; a pin dropped on the border of the action sits on it, is held by
    // it, follows it when it is moved or resized, and is let go when it is dragged far from every border; the starting point opens.
    {
        use crate::logic::library;
        cx.simulate_keystrokes("secondary-shift-d");
        cx.run_until_parked();
        let (act, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
        let drawn = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().clone());
        let family = tr("Activity", "Activité");
        let preset = |form: diagram::Form| library::presets().iter().position(|p| p.family == family && p.form == form).unwrap();
        let drop = |cx: &mut gpui::VisualTestContext, form: diagram::Form, x: f32, y: f32| {
            let at = canvas.read_with(cx, |c, _| c.spot(x, y));
            canvas.update(cx, |c, cx| c.drop_preset(preset(form), at, cx));
        };
        drop(cx, diagram::Form::Row, 150., 100.);
        drop(cx, diagram::Form::Round, 180., 100.);
        let d = drawn(cx);
        let by = |d: &Diagram, form: diagram::Form| d.shapes.iter().find(|s| s.form == form).unwrap().clone();
        let (row, action) = (by(&d, diagram::Form::Row), by(&d, diagram::Form::Round));
        assert_eq!((action.parent, row.w, row.h), (Some(row.id), 520., 130.));
        // The pin, let go on the right border of the action: on it, whatever the side it was let go on, and held by it.
        drop(cx, diagram::Form::Pin, 255., 104.);
        let d = drawn(cx);
        let pin = by(&d, diagram::Form::Pin);
        assert_eq!((pin.parent, pin.center()), (Some(action.id), (250., 107.)), "{pin:?}");
        // Dragged with the action, it goes with it, and stays held; the action stays in its row. The bar over
        // the selected pin could cover the action: nothing is selected when the drag begins.
        cx.simulate_keystrokes("escape");
        rows(cx, &canvas, (150., 85.), (150., 125.));
        let d = drawn(cx);
        let (moved, pin) = (by(&d, diagram::Form::Round), by(&d, diagram::Form::Pin));
        assert_eq!((moved.y, pin.center(), pin.parent, moved.parent), (action.y + 40., (250., 147.), Some(action.id), Some(row.id)));
        // Resized by its corner, the pin stays on the border.
        rows(cx, &canvas, (250., moved.y + moved.h), (210., moved.y + moved.h));
        let d = drawn(cx);
        let (resized, pin) = (by(&d, diagram::Form::Round), by(&d, diagram::Form::Pin));
        assert_eq!((resized.w, pin.center().0, pin.parent), (100., 210., Some(action.id)), "{resized:?} {pin:?}");
        // Dragged far from every border, it is let go; one step of undo puts it back on the border.
        rows(cx, &canvas, pin.center(), (210., 10.));
        let d = drawn(cx);
        let pin = by(&d, diagram::Form::Pin);
        assert!(pin.parent.is_none() && pin.center().0 == 210. && pin.center().1 < 20., "{pin:?}");
        cx.simulate_keystrokes("secondary-z");
        let pin = by(&drawn(cx), diagram::Form::Pin);
        assert_eq!((pin.parent, pin.center()), (Some(action.id), (210., 147.)));
        // Removing the action removes its pin with it; one step of undo brings back both.
        let before = drawn(cx);
        rows(cx, &canvas, (150., 125.), (150., 125.));
        cx.simulate_keystrokes("delete");
        let gone = drawn(cx);
        assert!(gone.shapes.iter().all(|s| s.form == diagram::Form::Row), "{:?}", gone.shapes);
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(drawn(cx).shapes.len(), before.shapes.len());
        // The file has them, and reads back the same.
        let text = saved(&act);
        assert!(text.contains("version 9\n") && text.contains(" pin ") && text.contains(" row "), "{text}");
        assert!(Diagram::from_svg(&text).unwrap() == drawn(cx));
        cx.simulate_keystrokes("secondary-a delete escape escape");
        cx.run_until_parked();
        fs::remove_file(&act).unwrap();
    }
    // Components and deployment: a device (a 3D box) holds the artifact dropped on it and takes it along; a port sits on the border of
    // a component; `K` then `J` draws a «deploy» arrow, and `V` makes it a «manifest».
    {
        use crate::logic::{library, relation::Relation};
        cx.simulate_keystrokes("secondary-shift-d");
        cx.run_until_parked();
        let (dep, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
        let drawn = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().clone());
        let family = tr("Component", "Composant");
        let preset = |name: &str| library::presets().iter().position(|p| p.family == family && p.name == name).unwrap();
        let drop = |cx: &mut gpui::VisualTestContext, name: &str, x: f32, y: f32| {
            let at = canvas.read_with(cx, |c, _| c.spot(x, y));
            canvas.update(cx, |c, cx| c.drop_preset(preset(name), at, cx));
        };
        drop(cx, tr("Device", "Appareil"), 100., 120.);
        drop(cx, tr("Artifact", "Artefact"), 100., 160.);
        drop(cx, tr("Component", "Composant"), 360., 100.);
        let d = drawn(cx);
        let by = |d: &Diagram, form: diagram::Form| d.shapes.iter().find(|s| s.form == form).unwrap().clone();
        let (node, artifact, component) = (by(&d, diagram::Form::Node), by(&d, diagram::Form::Rect), by(&d, diagram::Form::Component));
        assert_eq!((artifact.parent, component.parent), (Some(node.id), None));
        assert!(node.text.starts_with("«device»") && artifact.text.starts_with("«artifact»"));
        // The port, let go a little inside the left border of the component, is on it.
        drop(cx, tr("Port (on the border of a component)", "Port (sur le bord d'un composant)"), component.x + 4., component.y + component.h / 2.);
        let port = by(&drawn(cx), diagram::Form::Pin);
        assert_eq!((port.parent, port.center().0), (Some(component.id), component.x));
        // `K` then `J`: a dashed arrow with its stereotype, from the artifact to the component; then `V` changes it.
        cx.simulate_input("k");
        cx.simulate_input("j");
        rows(cx, &canvas, (100., 160.), (400., 100.));
        let d = drawn(cx);
        let arrow = d.links.last().unwrap();
        assert_eq!((Relation::of(arrow), arrow.text.as_str(), arrow.dashed), (Some(Relation::Deploy), "«deploy»", true));
        cx.simulate_keystrokes("escape");
        let id = arrow.id;
        let (mx, my) = (262., 123.);
        rows(cx, &canvas, (mx, my), (mx, my));
        cx.simulate_input("k");
        cx.simulate_input("v");
        let arrow = drawn(cx).links.last().unwrap().clone();
        assert_eq!((arrow.id, Relation::of(&arrow), arrow.text.as_str()), (id, Some(Relation::Manifest), "«manifest»"));
        // The device dragged by its front face takes its artifact along, the 3D box and what it holds.
        cx.simulate_keystrokes("escape");
        rows(cx, &canvas, (-30., 60.), (10., 60.));
        let d = drawn(cx);
        let (moved, held) = (by(&d, diagram::Form::Node), by(&d, diagram::Form::Rect));
        assert_eq!((moved.x, held.x, held.parent), (node.x + 40., artifact.x + 40., Some(node.id)));
        let text = saved(&dep);
        assert!(text.contains("version 10\n") && text.contains(" node ") && text.contains(" component "), "{text}");
        assert!(Diagram::from_svg(&text).unwrap() == d);
        cx.simulate_keystrokes("secondary-a delete escape escape");
        cx.run_until_parked();
        fs::remove_file(&dep).unwrap();
    }
    // From Mermaid to the canvas: the diagram shown large becomes a diagram of the vault, next to the note, which is not touched.
    {
        use crate::views::viewer::Source;
        let note = shell.read_with(cx, |s, _| s.pane().path.clone());
        let before = note.as_ref().map(|n| fs::read_to_string(n).unwrap_or_default());
        let show = |cx: &mut gpui::VisualTestContext, text: &str| {
            shell.update_in(cx, |s, window, cx| s.show_figure(Source::Mermaid(text.to_string()), window, cx));
            cx.run_until_parked();
        };
        let ask = |cx: &mut gpui::VisualTestContext| {
            let figure = shell.read_with(cx, |s, _| s.pane().figure.clone()).unwrap();
            figure.update(cx, |f, cx| f.edit_on_canvas(cx));
            cx.run_until_parked();
        };
        // A flowchart is not one of the three: nothing is made, and the figure stays.
        let files = |root: &Path| fs::read_dir(root).unwrap().count();
        let root = shell.read_with(cx, |s, _| s.vault.clone().unwrap());
        let count = files(&root);
        show(cx, "flowchart LR\nA --> B");
        ask(cx);
        assert!(shell.read_with(cx, |s, _| s.pane().figure.is_some() && s.pane().drawing().is_none()));
        assert_eq!(files(&root), count);
        // A sequence diagram, one line of which is not understood.
        show(cx, "sequenceDiagram\nA->>B: hi\nB-->>A: ho\nNote over A,B: x");
        ask(cx);
        let (path, canvas, figure) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned(), s.pane().drawing().cloned(), s.pane().figure.is_some()));
        let (path, canvas) = (path.unwrap(), canvas.unwrap());
        assert!(!figure, "the diagram takes the figure's place");
        let d = canvas.read_with(cx, |c, _| c.diagram().clone());
        assert_eq!((d.shapes.len(), d.messages().len()), (2, 2));
        assert_eq!(d.shapes[0].text, "A");
        assert!(path.starts_with(&root) && path.extension().is_some_and(|e| e == "svg") && path.file_name().unwrap().to_string_lossy().contains(tr("diagram", "schéma")), "{path:?}");
        assert_eq!(Diagram::from_svg(&fs::read_to_string(&path).unwrap()).unwrap(), d);
        assert_eq!(files(&root), count + 1);
        // What was not understood is said: one line.
        assert!(shell.read_with(cx, |s, _| s.toasts.iter().any(|t| t.text.starts_with("1 ") && t.text.contains(tr("not understood", "pas été comprise")))));
        // The same again makes a second file under another name: nothing is overwritten.
        show(cx, "sequenceDiagram\nA->>B: hi\nB-->>A: ho");
        ask(cx);
        let second = shell.read_with(cx, |s, _| s.pane().file_path().cloned()).unwrap();
        assert_ne!(second, path);
        assert_eq!(files(&root), count + 2);
        fs::remove_file(&second).unwrap();
        if let Some(note) = &note {
            assert_eq!(&fs::read_to_string(note).unwrap_or_default(), before.as_ref().unwrap());
        }
        cx.simulate_keystrokes("escape escape");
        cx.run_until_parked();
        fs::remove_file(&path).unwrap();
    }
    // A picture or a diagram selected in the tree is not a folder: what is created from the tree goes beside it, not "inside" it.
    {
        let root = shell.read_with(cx, |s, _| s.vault.clone().unwrap());
        let drawing = root.join("Dessin choisi.svg");
        fs::write(&drawing, Diagram::blank().to_svg(diagram::COLORS[0])).unwrap();
        let before: std::collections::HashSet<_> = fs::read_dir(&root).unwrap().flatten().map(|e| e.path()).collect();
        shell.update_in(cx, |s, window, cx| {
            s.nav.sel = Some(drawing.clone());
            s.nav.mode = crate::views::nav::Mode::Tree;
            s.nav.panel = crate::views::nav::Panel::Split;
            s.new_diagram(window, cx);
        });
        cx.run_until_parked();
        let made: Vec<_> = fs::read_dir(&root).unwrap().flatten().map(|e| e.path()).filter(|p| !before.contains(p)).collect();
        assert!(made.len() == 1 && made[0].extension().is_some_and(|e| e == "svg") && made[0].is_file(), "{made:?}");
        // The same for a new note.
        shell.update_in(cx, |s, window, cx| {
            s.nav.sel = Some(drawing.clone());
            s.new_note_in(s.nav.target_dir(), window, cx);
        });
        cx.run_until_parked();
        cx.simulate_keystrokes("escape escape");
        cx.run_until_parked();
        fs::remove_file(&drawing).unwrap();
        fs::remove_file(&made[0]).unwrap();
        shell.update(cx, |s, _| s.nav.sel = None);
    }
    // Dragged from the bar or from the library, a shape lands centred on the pointer, and what is drawn under the pointer
    // while it is dragged is centred on it too.
    {
        use crate::logic::library;
        cx.simulate_keystrokes("secondary-shift-d");
        cx.run_until_parked();
        let (drag, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
        let centre = |cx: &mut gpui::VisualTestContext, selector: &'static str| cx.debug_bounds(selector).map(|b| b.center());
        // From the dock: the rectangle, in the menu of shapes.
        let opener = centre(cx, "d-shapes").expect("the shapes button");
        cx.simulate_click(opener, gpui::Modifiers::none());
        cx.run_until_parked();
        let from = centre(cx, "d-tool-r").expect("the rectangle row");
        let to = canvas.read_with(cx, |c, _| c.spot(250., 150.));
        cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(from + gpui::point(gpui::px(10.), gpui::px(10.)), MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
        let ghost = |cx: &mut gpui::VisualTestContext| {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
            cx.debug_bounds("d-placing").map(|b| b.center())
        };
        let bar_ghost = ghost(cx);
        cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        let d = canvas.read_with(cx, |c, _| c.diagram().clone());
        let spot = |cx: &mut gpui::VisualTestContext, s: &diagram::Shape| canvas.read_with(cx, |c, _| c.spot(s.x + s.w / 2., s.y + s.h / 2.));
        let rect = spot(cx, &d.shapes[0]);
        let near = |a: gpui::Point<gpui::Pixels>, b: gpui::Point<gpui::Pixels>| f32::from((a.x - b.x).abs()).max(f32::from((a.y - b.y).abs()));
        assert!(near(rect, to) <= 5., "bar: the shape is at {rect:?}, the pointer at {to:?}");
        if let Some(g) = bar_ghost {
            assert!(near(g, to) <= 3., "bar: the picture under the pointer is at {g:?}, the pointer at {to:?}, the button was at {from:?}, bounds {:?}", cx.debug_bounds("d-placing"));
        }
        // From the library: a class, from the middle of its row, and from the left end of it.
        cx.simulate_input("s");
        let class = library::presets().iter().position(|p| p.form == diagram::Form::Rect && p.text.starts_with("NomDeClasse") || p.text.starts_with("ClassName")).unwrap();
        for (k, grab) in [0.5f32, 0.05].into_iter().enumerate() {
            let selector: &'static str = Box::leak(format!("d-preset-{class}").into_boxed_str());
            let bounds = cx.debug_bounds(selector).expect("the class row");
            let from = gpui::point(bounds.origin.x + bounds.size.width * grab, bounds.center().y);
            let to = canvas.read_with(cx, |c, _| c.spot(250., 150. + 80. * (k as f32 + 1.)));
            cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
            cx.simulate_mouse_move(from + gpui::point(gpui::px(10.), gpui::px(10.)), MouseButton::Left, gpui::Modifiers::none());
            cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
            let library_ghost = ghost(cx);
            cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
            cx.run_until_parked();
            let d = canvas.read_with(cx, |c, _| c.diagram().clone());
            let shape = d.shapes.last().unwrap().clone();
            let at = spot(cx, &shape);
            assert!(near(at, to) <= 5., "library, grab {grab}: the shape is at {at:?}, the pointer at {to:?}");
            if let Some(g) = library_ghost {
                assert!(near(g, to) <= 3., "library, grab {grab}: the picture under the pointer is at {g:?}, the pointer at {to:?}");
            }
            // The library is still open for the next one.
            if k == 0 {
                cx.simulate_input("s");
            }
        }
        cx.simulate_keystrokes("escape secondary-a delete escape escape");
        cx.run_until_parked();
        fs::remove_file(&drag).unwrap();
    }
    // The chrome of the canvas: a dock at the bottom, menus that name what they hold, a bar over the selection.
    {
        use crate::logic::relation::Relation;
        use gpui::{Bounds, Pixels};
        cx.simulate_keystrokes("secondary-shift-d");
        cx.run_until_parked();
        let (chrome, canvas) = shell.read_with(cx, |s, _| (s.pane().file_path().cloned().unwrap(), s.pane().drawing().cloned().unwrap()));
        let window = cx.update(|w, _| w.viewport_size());
        let bounds = |cx: &mut gpui::VisualTestContext, selector: String| -> Option<Bounds<Pixels>> {
            let selector: &'static str = Box::leak(selector.into_boxed_str());
            // What is not drawn any more stays in the table until the next frame.
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
            cx.debug_bounds(selector)
        };
        let inside = |b: Bounds<Pixels>| b.origin.x >= gpui::px(0.) && b.origin.y >= gpui::px(0.) && b.origin.x + b.size.width <= window.width && b.origin.y + b.size.height <= window.height;
        let click = |cx: &mut gpui::VisualTestContext, selector: String| {
            let at = bounds(cx, selector.clone()).unwrap_or_else(|| panic!("{selector} is not on the screen")).center();
            cx.simulate_click(at, gpui::Modifiers::none());
            cx.run_until_parked();
        };
        // The dock: one row, in the lower half of the window, nothing on anything else.
        let dock: Vec<Bounds<Pixels>> = ["d-tool-v", "d-shapes", "d-relations", "d-tool-t", "d-library"].iter().map(|s| bounds(cx, s.to_string()).unwrap_or_else(|| panic!("{s}: no button"))).collect();
        assert!(dock.iter().all(|b| inside(*b) && b.center().y > window.height / 2. && (b.center().y - dock[0].center().y).abs() < gpui::px(2.)), "{dock:?}");
        assert!(dock.iter().enumerate().all(|(i, a)| dock.iter().skip(i + 1).all(|b| !a.intersects(b))), "{dock:?}");
        // The menu of shapes names ten, one under the other, inside the window, and picking one is the tool.
        click(cx, "d-shapes".into());
        let keys = ["r", "u", "o", "d", "c", "p", "n", "g", "f", "w"];
        let rows: Vec<Bounds<Pixels>> = keys.iter().map(|k| bounds(cx, format!("d-tool-{k}")).unwrap_or_else(|| panic!("no row for {k}"))).collect();
        assert!(rows.iter().all(|b| inside(*b)) && rows.windows(2).all(|w| w[0].origin.y + w[0].size.height <= w[1].origin.y + gpui::px(0.5)), "{rows:?}");
        click(cx, "d-tool-o".into());
        assert_eq!(canvas.read_with(cx, |c, _| c.tool()), canvas::Tool::Shape(diagram::Form::Ellipse));
        // (The bounds of what is not drawn any more stay in the table: the state is what is read.)
        let chrome_of = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.chrome());
        assert_eq!(chrome_of(cx), (false, false, false), "the menu closes once a shape is picked");
        // The menu of relationships: the 24 of them and the n-ary association, none on another, all in the window.
        cx.simulate_keystrokes("escape");
        click(cx, "d-relations".into());
        let all: Vec<Bounds<Pixels>> = Relation::ALL.iter().map(|r| bounds(cx, format!("d-relation-{}", *r as usize)).unwrap_or_else(|| panic!("no row for {r:?}"))).collect();
        assert!(all.iter().all(|b| inside(*b)), "{all:?}");
        assert!(all.iter().enumerate().all(|(i, a)| all.iter().skip(i + 1).all(|b| !a.intersects(b))), "rows overlap");
        click(cx, format!("d-relation-{}", Relation::Generalization as usize));
        assert_eq!(canvas.read_with(cx, |c, _| c.tool()), canvas::Tool::Link(Relation::Generalization));
        assert_eq!(chrome_of(cx), (false, false, false));
        // Escape drops the tool; a menu that is open closes with it first.
        click(cx, "d-shapes".into());
        assert!(chrome_of(cx).0);
        cx.simulate_keystrokes("escape");
        assert_eq!(chrome_of(cx), (false, false, false));
        // Two shapes and an arrow between them: the arrow is selected, and a bar floats over it.
        let drag = |cx: &mut gpui::VisualTestContext, from: (f32, f32), to: (f32, f32)| {
            let (from, to) = canvas.read_with(cx, |c, _| (c.spot(from.0, from.1), c.spot(to.0, to.1)));
            cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
            cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
            cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
        };
        cx.simulate_input("r");
        drag(cx, (0., 0.), (120., 60.));
        cx.simulate_keystrokes("escape");
        cx.simulate_input("o");
        drag(cx, (300., 0.), (420., 80.));
        cx.simulate_keystrokes("escape");
        cx.simulate_input("a");
        drag(cx, (60., 30.), (350., 40.));
        let bar = bounds(cx, "d-selection-bar".into()).expect("a bar over the selected arrow");
        assert!(inside(bar) && dock.iter().all(|b| !bar.intersects(b)), "{bar:?} {dock:?}");
        let chip = bounds(cx, "d-relation-chip".into()).expect("the chip that names the relationship");
        assert!(bar.contains(&chip.center()));
        // Naming the relationship opens the menu; a row of it changes the arrow.
        click(cx, "d-relation-chip".into());
        click(cx, format!("d-relation-{}", Relation::Realization as usize));
        let link = canvas.read_with(cx, |c, _| c.diagram().links[0].clone());
        assert_eq!((link.start, link.end, link.dashed), Relation::Realization.look());
        assert_eq!(canvas.read_with(cx, |c, _| c.tool()), canvas::Tool::Select);
        // In a narrow window the bar wraps, and bar and dock stay inside it.
        for width in [420., 700.] {
            cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(600.)));
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
            let narrow = cx.update(|w, _| w.viewport_size());
            let (bar, dock) = (bounds(cx, "d-selection-bar".into()).unwrap(), bounds(cx, "d-library".into()).unwrap());
            let fits = |b: Bounds<Pixels>| b.origin.x >= gpui::px(0.) && b.origin.y >= gpui::px(0.) && b.origin.x + b.size.width <= narrow.width && b.origin.y + b.size.height <= narrow.height;
            assert!(fits(bar) && fits(dock), "{width}: bar {bar:?}, dock {dock:?}, window {narrow:?}");
            let menu = bounds(cx, "d-delete".into()).unwrap();
            assert!(bar.contains(&menu.center()) && fits(menu), "{width}: the last button of the bar is outside it: {menu:?}");
            // The zoom is not on the dock.
            let zoom = [bounds(cx, "d-zoom-in".into()).unwrap(), bounds(cx, "d-zoom-out".into()).unwrap()];
            let row: Vec<_> = ["d-tool-v", "d-shapes", "d-relations", "d-tool-t", "d-library"].iter().map(|s| bounds(cx, s.to_string()).unwrap()).collect();
            assert!(zoom.iter().all(|z| fits(*z) && row.iter().all(|d| !z.intersects(d))), "{width}: the zoom {zoom:?} is on the dock {row:?}");
        }
        cx.simulate_resize(window);
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        // While a shape is dragged the bar is out of the way; let go, it comes back.
        let (from, to) = canvas.read_with(cx, |c, _| (c.spot(20., 52.), c.spot(20., 132.)));
        cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        assert!(!chrome_of(cx).2, "the bar hides while a shape is dragged");
        cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        assert!(chrome_of(cx).2);
        // Nothing selected, no bar.
        cx.simulate_keystrokes("escape");
        assert!(!chrome_of(cx).2);
        // Pointing at a shape lights it and shows the handles an arrow is drawn from; dragging one to another shape
        // draws it, with the look of the last arrow picked (the key `A` made it a plain arrow).
        let (rect, ellipse) = canvas.read_with(cx, |c, _| (c.diagram().shapes[0].clone(), c.diagram().shapes[1].clone()));
        let (before, over) = canvas.read_with(cx, |c, _| (c.diagram().links.len(), c.spot(rect.x + rect.w / 2., rect.y + rect.h / 2.)));
        cx.simulate_mouse_move(over, None, gpui::Modifiers::none());
        assert_eq!(canvas.read_with(cx, |c, _| c.pointed().0), Some(rect.id));
        let handle = canvas.read_with(cx, |c, _| c.handle_spot(rect.id, 1)).unwrap();
        cx.simulate_mouse_move(handle, None, gpui::Modifiers::none());
        assert_eq!(canvas.read_with(cx, |c, _| c.pointed().0), Some(rect.id), "the handle is part of the shape it belongs to");
        let target = canvas.read_with(cx, |c, _| c.spot(ellipse.x + ellipse.w / 2., ellipse.y + ellipse.h / 2.));
        cx.simulate_mouse_down(handle, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(target, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(target, MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        let drawn = canvas.read_with(cx, |c, _| c.diagram().clone());
        let made = drawn.links.last().unwrap();
        assert_eq!(drawn.links.len(), before + 1);
        assert_eq!((made.from, made.end), (diagram::End::Pin(rect.id, 1., 0.5), diagram::Head::Arrow), "{made:?}");
        assert_eq!(made.to.shape(), Some(ellipse.id));
        // Nothing under the pointer: nothing is lit.
        let far = canvas.read_with(cx, |c, _| c.spot(430., 220.));
        cx.simulate_mouse_move(far, None, gpui::Modifiers::none());
        assert_eq!(canvas.read_with(cx, |c, _| c.pointed().0), None);
        // A right click on a shape selects it and opens its menu; a row of it acts, and the menu closes.
        let right = |cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>| {
            cx.simulate_mouse_down(at, MouseButton::Right, gpui::Modifiers::none());
            cx.simulate_mouse_up(at, MouseButton::Right, gpui::Modifiers::none());
            cx.run_until_parked();
        };
        let shapes = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().shapes.len());
        let count = shapes(cx);
        right(cx, over);
        assert_eq!(canvas.read_with(cx, |c, _| (c.pointed().1, c.selected_ids())), (true, vec![rect.id]));
        let menu = bounds(cx, "d-context-menu".into()).expect("the menu of a right click");
        assert!(inside(menu));
        click(cx, "d-cm-duplicate".into());
        assert_eq!((shapes(cx), canvas.read_with(cx, |c, _| c.pointed().1)), (count + 1, false));
        // On an arrow: turn it around.
        let (link, mid) = canvas.read_with(cx, |c, _| (c.diagram().links.last().unwrap().clone(), c.diagram().mid(c.diagram().links.last().unwrap().id).unwrap()));
        let on = canvas.read_with(cx, |c, _| c.spot(mid.0, mid.1));
        right(cx, on);
        click(cx, "d-cm-reverse".into());
        let turned = canvas.read_with(cx, |c, _| c.diagram().link(link.id).unwrap().clone());
        assert_eq!((turned.from, turned.to, turned.start, turned.end), (link.to, link.from, link.end, link.start));
        // On the empty canvas: a text, written at once, where the click was; Esc closes a menu.
        right(cx, far);
        assert!(bounds(cx, "d-cm-text".into()).is_some());
        cx.simulate_keystrokes("escape");
        assert!(!canvas.read_with(cx, |c, _| c.pointed().1));
        right(cx, far);
        click(cx, "d-cm-text".into());
        let text = canvas.read_with(cx, |c, _| c.diagram().shapes.last().unwrap().clone());
        assert_eq!(text.form, diagram::Form::Text);
        cx.simulate_keystrokes("escape");
        // The bar keeps clear of the handles of the shape it is over, so that an arrow can be drawn from it.
        click(cx, "d-tool-v".into());
        let on_rect = canvas.read_with(cx, |c, _| c.spot(rect.x + 10., rect.y + rect.h - 10.));
        cx.simulate_click(on_rect, gpui::Modifiers::none());
        cx.run_until_parked();
        let bar = bounds(cx, "d-selection-bar".into()).expect("the bar over the rectangle");
        for side in 0..4 {
            let handle = canvas.read_with(cx, |c, _| c.handle_spot(rect.id, side)).unwrap();
            assert!(!bar.contains(&handle), "the bar {bar:?} covers the handle of side {side} at {handle:?}");
        }
        // A click on a handle changes nothing, and is no step of undo.
        let handle = canvas.read_with(cx, |c, _| c.handle_spot(rect.id, 2)).unwrap();
        cx.simulate_mouse_move(on_rect, None, gpui::Modifiers::none());
        cx.simulate_mouse_move(handle, None, gpui::Modifiers::none());
        let (_, depth) = canvas.read_with(cx, |c, _| c.gesture());
        cx.simulate_click(handle, gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(canvas.read_with(cx, |c, _| c.gesture()), (false, depth));
        // K opens and closes the menu of relationships, and opening one closes the other.
        cx.simulate_input("k");
        assert!(chrome_of(cx).1);
        cx.simulate_input("k");
        assert_eq!((chrome_of(cx).0, chrome_of(cx).1), (false, false));
        click(cx, "d-shapes".into());
        cx.simulate_input("k");
        assert_eq!((chrome_of(cx).0, chrome_of(cx).1), (false, true));
        // A click on the canvas closes the menus.
        cx.simulate_click(far, gpui::Modifiers::none());
        assert_eq!((chrome_of(cx).0, chrome_of(cx).1), (false, false));
        // With a menu open, the arrow keys are not for the selection.
        cx.simulate_click(on_rect, gpui::Modifiers::none());
        right(cx, on_rect);
        let x = canvas.read_with(cx, |c, _| c.diagram().shape(rect.id).unwrap().x);
        cx.simulate_keystrokes("right");
        assert_eq!(canvas.read_with(cx, |c, _| c.diagram().shape(rect.id).unwrap().x), x);
        cx.simulate_keystrokes("escape");
        // Fill and dashes agree on a mixed selection: on, unless all are on already.
        let fills = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().shapes.iter().map(|s| s.fill).collect::<Vec<_>>());
        cx.simulate_keystrokes("secondary-a");
        click(cx, "d-fill".into());
        assert!(fills(cx).iter().all(|f| *f), "{:?}", fills(cx));
        cx.simulate_keystrokes("escape");
        cx.simulate_click(on_rect, gpui::Modifiers::none());
        click(cx, "d-fill".into());
        assert_eq!(fills(cx).iter().filter(|f| !**f).count(), 1);
        cx.simulate_keystrokes("secondary-a");
        click(cx, "d-fill".into());
        assert!(fills(cx).iter().all(|f| *f), "a mixed selection is all filled, not turned over: {:?}", fills(cx));
        // Let go over the dock, which hides the pointer from the canvas: the gesture ends there all the same.
        cx.simulate_keystrokes("escape");
        let dock = bounds(cx, "d-tool-v".into()).unwrap().center();
        let (from, to) = canvas.read_with(cx, |c, _| (c.spot(120., 250.), c.spot(240., 300.)));
        cx.simulate_input("r");
        cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(dock, MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        assert!(!canvas.read_with(cx, |c, _| c.gesture().0), "the drag is still on after the button was let go over the dock");
        let last = canvas.read_with(cx, |c, _| c.diagram().shapes.last().unwrap().clone());
        assert_eq!((last.w, last.h), (120., 50.), "{last:?}");
        cx.simulate_keystrokes("escape");
        // The menus are walked with the keys: down, up and Enter pick the row the cursor is on.
        click(cx, "d-shapes".into());
        cx.simulate_keystrokes("down down enter");
        assert_eq!(canvas.read_with(cx, |c, _| c.tool()), canvas::Tool::Shape(diagram::Form::Ellipse));
        cx.simulate_keystrokes("escape");
        cx.simulate_input("k");
        cx.simulate_keystrokes("down enter");
        assert_eq!(canvas.read_with(cx, |c, _| c.tool()), canvas::Tool::Link(Relation::Navigable));
        cx.simulate_keystrokes("escape");
        cx.simulate_input("k");
        cx.simulate_keystrokes("up enter");
        assert_eq!(canvas.read_with(cx, |c, _| c.tool()), canvas::Tool::Link(Relation::Lost), "the cursor goes round: up from the first row is the last");
        cx.simulate_keystrokes("escape");
        // In the menu of a right click: delete is the third row of a shape's.
        let count = shapes(cx);
        right(cx, on_rect);
        cx.simulate_keystrokes("down down enter");
        assert_eq!((shapes(cx), canvas.read_with(cx, |c, _| c.pointed().1)), (count - 1, false));
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(shapes(cx), count);
        // In the library, typing narrows it and the keys pick.
        cx.simulate_input("s");
        cx.simulate_input("interface");
        cx.simulate_keystrokes("enter");
        let first = match canvas.read_with(cx, |c, _| c.tool()) {
            canvas::Tool::Preset(i) => i,
            other => panic!("{other:?}"),
        };
        assert!(crate::logic::library::presets()[first].name.to_lowercase().contains("interface"));
        cx.simulate_keystrokes("escape");
        cx.simulate_input("s");
        cx.simulate_input("interface");
        cx.simulate_keystrokes("down enter");
        let second = match canvas.read_with(cx, |c, _| c.tool()) {
            canvas::Tool::Preset(i) => i,
            other => panic!("{other:?}"),
        };
        let answers = crate::logic::library::presets().iter().filter(|p| p.matches("interface")).count();
        assert!(answers < 2 || second != first, "down did not move to another row: {first} {second}");
        cx.simulate_keystrokes("escape");
        // 1 to 6 colour the selection.
        cx.simulate_click(on_rect, gpui::Modifiers::none());
        cx.simulate_input("3");
        assert_eq!(canvas.read_with(cx, |c, _| c.diagram().shape(rect.id).unwrap().color), 2);
        cx.simulate_input("1");
        assert_eq!(canvas.read_with(cx, |c, _| c.diagram().shape(rect.id).unwrap().color), 0);
        cx.simulate_keystrokes("escape");
        fs::remove_file(&chrome).unwrap();
        cx.simulate_keystrokes("escape escape");
        cx.run_until_parked();
    }
    fs::remove_file(&styled).unwrap();
    j.settle(cx);
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
}
