//! Writing in a diagram: the text of a shape or an arrow is edited like text, by displayed character
//! (a joined emoji, a flag, a skin tone or a decomposed accent never leaves a fragment behind), with
//! the mouse (cursor, drag, Shift+click, double click), the keys (lines, words, selection) and the
//! clipboard.

use super::*;
use crate::logic::diagram::{Diagram, End, Head, Route, Slot};
use gpui::{Modifiers, MouseButton};

#[gpui::test]
fn label_editing_goes_by_displayed_character(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "canvas-text");
    let file = root.join("Schéma.svg");
    fs::write(&file, "<svg><metadata id=\"bref-diagram\">\nversion 1\n</metadata></svg>\n").unwrap();
    let canvas = shell.update_in(cx, |s, window, cx| {
        s.load_note(&file, cx);
        let canvas = s.pane().drawing().cloned().unwrap();
        window.focus(&canvas.focus_handle(cx));
        canvas
    });
    cx.run_until_parked();
    cx.simulate_input("r");
    let (from, to) = canvas.read_with(cx, |c, _| (c.spot(0., 0.), c.spot(120., 60.)));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    let label = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().shapes[0].text.clone());

    // The reported case: one Backspace takes the whole joined emoji.
    cx.simulate_keystrokes("enter");
    cx.simulate_input("A👩\u{200d}💻");
    cx.simulate_keystrokes("backspace");
    assert_eq!(label(cx), "A");
    // Two flags side by side are two characters: one Backspace takes one.
    cx.simulate_input("🇫🇷🇯🇵");
    cx.simulate_keystrokes("backspace");
    assert_eq!(label(cx), "A🇫🇷");
    // Empty text: nothing to move over or erase, no panic.
    cx.simulate_keystrokes("backspace backspace backspace left right delete");
    assert_eq!(label(cx), "");

    let clusters = ["👩\u{200d}💻", "👨\u{200d}👩\u{200d}👧\u{200d}👦", "🇫🇷", "👍🏽", "e\u{301}", "é", "家", "a"];
    for cluster in clusters {
        // Next to a newline, at the end, at the start, beside CJK.
        cx.simulate_input(&format!("{cluster}\n{cluster}x"));
        cx.simulate_keystrokes("backspace backspace");
        assert_eq!(label(cx), format!("{cluster}\n"), "{cluster}");
        cx.simulate_keystrokes("left left delete");
        assert_eq!(label(cx), "\n", "{cluster}");
        cx.simulate_keystrokes("right backspace");
        assert_eq!(label(cx), "", "{cluster}");
        cx.simulate_input(&format!("家{cluster}家"));
        cx.simulate_keystrokes("left left backspace");
        assert_eq!(label(cx), format!("{cluster}家"), "{cluster}");
        cx.simulate_keystrokes("right right backspace backspace");
        assert_eq!(label(cx), "", "{cluster}");
    }

    // What is left is saved whole, and undo and redo bring it back as it was.
    cx.simulate_input("Café 👩\u{200d}💻🇫🇷!");
    cx.simulate_keystrokes("backspace backspace escape");
    cx.run_until_parked();
    let saved = || Diagram::from_svg(&fs::read_to_string(&file).unwrap()).unwrap().shapes[0].text.clone();
    assert_eq!((label(cx), saved()), ("Café 👩\u{200d}💻".into(), "Café 👩\u{200d}💻".into()));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(label(cx), "");
    cx.simulate_keystrokes("secondary-shift-z");
    assert_eq!(label(cx), "Café 👩\u{200d}💻");
}

fn open(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext, path: &Path) -> Entity<crate::views::canvas::Canvas> {
    shell.update_in(cx, |s, window, cx| s.open_from_nav(path, window, cx));
    cx.run_until_parked();
    shell.read_with(cx, |s, _| s.pane().drawing().cloned().unwrap())
}

fn press(cx: &mut gpui::VisualTestContext, at: Point<Pixels>, modifiers: gpui::Modifiers, clicks: usize) {
    cx.simulate_event(gpui::MouseDownEvent { position: at, modifiers, button: MouseButton::Left, click_count: clicks, first_mouse: false });
    cx.simulate_event(gpui::MouseUpEvent { position: at, modifiers, button: MouseButton::Left, click_count: clicks });
    cx.run_until_parked();
}

#[gpui::test]
fn a_label_is_edited_like_text(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "canvas-text");
    let mut drawing = Diagram::blank();
    let rect = drawing.add_shape(diagram::Form::Rect, 0., 0., 160., 60.);
    drawing.shape_mut(rect).unwrap().text = "Alpha\nBeta".into();
    let path = root.join("Labels.svg");
    fs::write(&path, drawing.to_svg(diagram::COLORS[0])).unwrap();
    let canvas = open(&shell, cx, &path);
    let writing = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.label().map(|(t, r)| (t.to_string(), r)));
    let text = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().shape(rect).unwrap().text.clone());
    // A point on the line of byte `at` of the text being written, at the left of that character.
    let byte = |cx: &mut gpui::VisualTestContext, at: usize| canvas.read_with(cx, |c, _| c.spot_of_byte(at).unwrap()) + point(px(0.), px(8.));
    let clipboard = |cx: &mut gpui::VisualTestContext| cx.read_from_clipboard().and_then(|c| c.text());
    let centre = canvas.read_with(cx, |c, _| c.spot(80., 30.));
    press(cx, centre, gpui::Modifiers::none(), 1);
    cx.simulate_keystrokes("enter");
    assert_eq!(writing(cx), Some(("Alpha\nBeta".into(), 10..10)));

    // Up goes to the line above, Down comes back: what is typed lands there.
    cx.simulate_keystrokes("up");
    cx.simulate_input("!");
    let lines = text(cx);
    assert!(lines.split('\n').next().unwrap().contains('!') && lines.ends_with("\nBeta"), "{lines:?}");
    cx.simulate_keystrokes("backspace down end");
    cx.simulate_input("?");
    assert_eq!(text(cx), "Alpha\nBeta?");
    // Home and End keep to the line; the word key (Alt on macOS, Ctrl elsewhere) goes by word.
    cx.simulate_keystrokes("home");
    assert_eq!(writing(cx).unwrap().1, 6..6);
    let word = if cfg!(target_os = "macos") { "alt" } else { "ctrl" };
    cx.simulate_keystrokes(&format!("up end {word}-left"));
    assert_eq!(writing(cx).unwrap().1, 0..0);

    // Ctrl+A selects the text, not the diagram: what is typed replaces it.
    cx.simulate_keystrokes("secondary-a");
    assert_eq!(writing(cx).unwrap().1, 0..11);
    cx.simulate_input("X");
    assert_eq!(text(cx), "X");
    // Undo brings back the text as it was before writing, and writing ends.
    cx.simulate_keystrokes("escape secondary-z");
    assert_eq!((text(cx), writing(cx)), ("Alpha\nBeta".into(), None));

    // Shift+arrows select, Ctrl+C copies, Ctrl+X cuts, Ctrl+V pastes over the selection.
    press(cx, centre, gpui::Modifiers::none(), 1);
    cx.simulate_keystrokes("enter");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("seed".into()));
    cx.simulate_keystrokes("shift-left secondary-c");
    assert_eq!(clipboard(cx).as_deref(), Some("a"));
    cx.simulate_keystrokes("shift-left secondary-x");
    assert_eq!((clipboard(cx).as_deref(), text(cx).as_str()), (Some("ta"), "Alpha\nBe"));
    cx.simulate_keystrokes("shift-up");
    assert_eq!(writing(cx).unwrap().1.end, 8);
    cx.simulate_keystrokes("secondary-v");
    assert!(text(cx).ends_with("ta") && !text(cx).contains('\n'), "{:?}", text(cx));
    cx.simulate_keystrokes("secondary-a");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("Alpha\r\nBeta".into()));
    cx.simulate_keystrokes("secondary-v");
    assert_eq!(text(cx), "Alpha\nBeta");

    // A click in the text puts the cursor there, and writing goes on.
    let at = byte(cx, 2);
    press(cx, at, gpui::Modifiers::none(), 1);
    assert_eq!(writing(cx).unwrap().1, 2..2);
    cx.simulate_input("Z");
    assert_eq!(text(cx), "AlZpha\nBeta");
    // A click in the shape, away from the text, too: the nearest place in it.
    press(cx, canvas.read_with(cx, |c, _| c.spot(4., 56.)), gpui::Modifiers::none(), 1);
    assert!(writing(cx).is_some_and(|(_, r)| r.is_empty() && r.start >= 7), "{:?}", writing(cx));
    // Dragging selects; Shift+click reaches further; a double click takes a word, a triple click all.
    let (from, to) = (byte(cx, 0), byte(cx, 3));
    cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    assert_eq!(writing(cx).unwrap().1, 0..3);
    let at = byte(cx, 9);
    press(cx, at, gpui::Modifiers::shift(), 1);
    assert_eq!(writing(cx).unwrap().1, 0..9);
    let at = byte(cx, 9);
    press(cx, at, gpui::Modifiers::none(), 2);
    assert_eq!(writing(cx).map(|(t, r)| t[r].to_string()).as_deref(), Some("Beta"));
    press(cx, at, gpui::Modifiers::none(), 3);
    assert_eq!(writing(cx).unwrap().1, 0..11);
    // Dragged up past the text, then out of the canvas and let go there: the selection reaches the
    // start of the text, the gesture ends.
    let (from, above, out) = (byte(cx, 7), canvas.read_with(cx, |c, _| c.spot(80., -30.)), point(px(-50.), px(-50.)));
    cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(above, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(out, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(out, MouseButton::Left, gpui::Modifiers::none());
    assert_eq!((writing(cx).unwrap().1, canvas.read_with(cx, |c, _| c.gesture().0)), (0..7, false));

    // Escape ends writing and keeps the text; it is on disk.
    cx.simulate_keystrokes("escape");
    assert_eq!(writing(cx), None);
    let saved = || Diagram::from_svg(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved().shape(rect).unwrap().text, "AlZpha\nBeta");
    // A click elsewhere ends writing too, and selects what is there.
    press(cx, centre, gpui::Modifiers::none(), 2);
    assert!(writing(cx).is_some());
    press(cx, canvas.read_with(cx, |c, _| c.spot(400., 300.)), gpui::Modifiers::none(), 1);
    assert_eq!((writing(cx), canvas.read_with(cx, |c, _| c.picked())), (None, 0));
    let _ = fs::remove_dir_all(root);
}

/// The cursor is where the glyphs are drawn: at each zoom, a centred line is around the middle of its
/// shape, a compartment of a class starts at its left, and an arrow's texts sit where they are drawn.
#[gpui::test]
fn the_cursor_follows_the_glyphs_in_classes_arrows_and_at_any_zoom(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "canvas-text-zoom");
    let mut drawing = Diagram::blank();
    drawing.rich = true;
    let class = drawing.add_shape(diagram::Form::Rect, 0., 0., 200., 120.);
    drawing.shape_mut(class).unwrap().text = "Client\n---\n+ nom: Texte\n---\n+ payer()".into();
    let other = drawing.add_shape(diagram::Form::Rect, 400., 0., 120., 60.);
    let link = drawing.add_link(End::Shape(class), End::Shape(other), Head::Arrow, Route::Straight);
    drawing.link_mut(link).unwrap().text = "paie\nchaque mois".into();
    drawing.link_mut(link).unwrap().start_text = "1".into();
    let path = root.join("Classes.svg");
    fs::write(&path, drawing.to_svg(diagram::COLORS[0])).unwrap();
    let canvas = open(&shell, cx, &path);
    let writing = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.label().map(|(t, r)| (t.to_string(), r)));
    let byte = |cx: &mut gpui::VisualTestContext, at: usize| canvas.read_with(cx, |c, _| c.spot_of_byte(at).unwrap());

    for zoom in ["-", "0", "+"] {
        // 20 % (fewer is refused), as fitted, then 400 % (more is refused); the view moved each time.
        for _ in 0..12 {
            cx.simulate_input(zoom);
        }
        let inside = canvas.read_with(cx, |c, _| c.spot(100., 60.));
        cx.simulate_event(gpui::ScrollWheelEvent { position: inside, delta: gpui::ScrollDelta::Pixels(point(px(37.), px(-21.))), modifiers: gpui::Modifiers::none(), touch_phase: gpui::TouchPhase::Moved });
        cx.run_until_parked();
        let scale = canvas.read_with(cx, |c, _| f32::from(c.spot(1., 0.).x - c.spot(0., 0.).x));
        press(cx, canvas.read_with(cx, |c, _| c.spot(100., 60.)), gpui::Modifiers::none(), 2);
        assert_eq!(writing(cx).map(|(t, _)| t), Some("Client\n---\n+ nom: Texte\n---\n+ payer()".into()), "zoom {scale}");
        // The title is centred on the class; a member starts 8 units in from its left edge.
        let (left, right, middle) = (byte(cx, 0), byte(cx, 6), canvas.read_with(cx, |c, _| c.spot(100., 0.)));
        assert!((f32::from(left.x + right.x) / 2. - f32::from(middle.x)).abs() < 1., "zoom {scale}: {left:?} {right:?} {middle:?}");
        let (member, edge) = (byte(cx, 11), canvas.read_with(cx, |c, _| c.spot(8., 0.)));
        assert!((f32::from(member.x - edge.x)).abs() < 0.5, "zoom {scale}");
        // A click on a member puts the cursor in it; Enter there adds a line to the compartment.
        let at = byte(cx, 13) + point(px(0.), px(3. * scale));
        press(cx, at, gpui::Modifiers::none(), 1);
        assert_eq!(writing(cx).unwrap().1, 13..13, "zoom {scale}");
        // Up skips the compartment's rule, to the title; Down comes back.
        cx.simulate_keystrokes("up");
        assert!(writing(cx).unwrap().1.end <= 6, "zoom {scale}");
        cx.simulate_keystrokes("down end enter");
        cx.simulate_input("+ age: Entier");
        assert_eq!(writing(cx).unwrap().0, "Client\n---\n+ nom: Texte\n+ age: Entier\n---\n+ payer()");
        cx.simulate_keystrokes("escape secondary-z");
    }

    // The text in the middle of an arrow, two lines, centred; then the one at its start.
    cx.simulate_input("0");
    cx.run_until_parked();
    // ponytail: started by its call rather than a double click on the stroke, whose place depends on the route.
    canvas.update(cx, |c, cx| c.write_in(link, Slot::Middle, cx));
    cx.run_until_parked();
    assert_eq!(writing(cx).map(|(t, _)| t), Some("paie\nchaque mois".into()));
    let at = byte(cx, 7) + point(px(0.), px(6.));
    press(cx, at, gpui::Modifiers::none(), 1);
    assert_eq!(writing(cx).unwrap().1, 7..7);
    cx.simulate_keystrokes("up");
    assert!(writing(cx).unwrap().1.end <= 4);
    cx.simulate_keystrokes("end enter");
    cx.simulate_input("ou");
    assert_eq!(writing(cx).unwrap().0, "paie\nou\nchaque mois");
    // Tab: the text at the end (none yet), then the one at the start.
    cx.simulate_keystrokes("tab");
    assert_eq!(writing(cx), Some(("".into(), 0..0)));
    cx.simulate_keystrokes("tab");
    assert_eq!(writing(cx), Some(("1".into(), 1..1)));
    let at = byte(cx, 0) + point(px(0.), px(6.));
    press(cx, at, gpui::Modifiers::none(), 1);
    assert_eq!(writing(cx).unwrap().1, 0..0);
    cx.simulate_input("0..");
    cx.simulate_keystrokes("escape");
    let saved = Diagram::from_svg(&fs::read_to_string(&path).unwrap()).unwrap();
    let saved = saved.link(link).unwrap();
    assert_eq!((saved.text.as_str(), saved.start_text.as_str()), ("paie\nou\nchaque mois", "0..1"));
    let _ = fs::remove_dir_all(root);
}

/// An input method composes in a label: each step replaces the previous one, underlined, until it is
/// committed or given up; its candidate window opens on the text, at any zoom; the UTF-16 ranges the
/// system sends land on the right characters, beside a two-unit emoji too.
#[gpui::test]
fn an_input_method_composes_in_a_label(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    let (root, shell, cx, _config) = start(cx, "canvas-ime");
    let mut drawing = Diagram::blank();
    let rect = drawing.add_shape(diagram::Form::Rect, 0., 0., 200., 60.);
    drawing.shape_mut(rect).unwrap().text = "Alpha\nBeta".into();
    let other = drawing.add_shape(diagram::Form::Rect, 400., 0., 120., 60.);
    let link = drawing.add_link(End::Shape(rect), End::Shape(other), Head::Arrow, Route::Straight);
    let path = root.join("Composition.svg");
    fs::write(&path, drawing.to_svg(diagram::COLORS[0])).unwrap();
    let canvas = open(&shell, cx, &path);
    let text = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().shape(rect).unwrap().text.clone());
    let marked = |cx: &mut gpui::VisualTestContext| cx.update(|window, cx| canvas.update(cx, |c, cx| c.marked_text_range(window, cx)));
    let compose = |cx: &mut gpui::VisualTestContext, range: Option<std::ops::Range<usize>>, s: &str| {
        cx.update(|window, cx| canvas.update(cx, |c, cx| c.replace_and_mark_text_in_range(range, s, None, window, cx)));
        cx.run_until_parked();
    };
    let commit = |cx: &mut gpui::VisualTestContext, range: Option<std::ops::Range<usize>>, s: &str| {
        cx.update(|window, cx| canvas.update(cx, |c, cx| c.replace_text_in_range(range, s, window, cx)));
        cx.run_until_parked();
    };
    let selection = |cx: &mut gpui::VisualTestContext| cx.update(|window, cx| canvas.update(cx, |c, cx| c.selected_text_range(false, window, cx).map(|s| s.range)));
    let read = |cx: &mut gpui::VisualTestContext, range: std::ops::Range<usize>| cx.update(|window, cx| canvas.update(cx, |c, cx| c.text_for_range(range, &mut None, window, cx)));
    let candidates = |cx: &mut gpui::VisualTestContext, at: usize| cx.update(|window, cx| canvas.update(cx, |c, cx| c.bounds_for_range(at..at, Bounds::default(), window, cx)));

    // Outside writing, a composition neither writes nor picks a tool.
    compose(cx, None, "r");
    assert_eq!((canvas.read_with(cx, |c, _| c.tool()), marked(cx), text(cx)), (crate::views::canvas::Tool::Select, None, "Alpha\nBeta".into()));

    press(cx, canvas.read_with(cx, |c, _| c.spot(100., 30.)), Modifiers::none(), 1);
    cx.simulate_keystrokes("enter");
    // Kana then kanji: each step replaces the last, by the marked range or by the range sent.
    compose(cx, None, "k");
    compose(cx, None, "か");
    assert_eq!((text(cx), marked(cx)), ("Alpha\nBetaか".into(), Some(10..11)));
    compose(cx, Some(10..11), "かん");
    assert_eq!((text(cx), marked(cx), selection(cx)), ("Alpha\nBetaかん".into(), Some(10..12), Some(12..12)));
    assert_eq!(read(cx, 10..12).as_deref(), Some("かん"));
    // The candidate window opens on the composed text, on its line (the second).
    let (spot, first) = canvas.read_with(cx, |c, _| (c.spot_of_byte(10).unwrap(), c.spot_of_byte(0).unwrap()));
    let window = candidates(cx, 10).unwrap();
    assert!((window.origin.x - spot.x).abs() < px(0.5) && window.origin.y == spot.y && spot.y > first.y, "{window:?} {spot:?}");
    commit(cx, None, "漢");
    assert_eq!((text(cx), marked(cx), selection(cx)), ("Alpha\nBeta漢".into(), None, Some(11..11)));

    // Given up: an empty step takes the provisional text away.
    compose(cx, None, "x");
    compose(cx, None, "");
    assert_eq!((text(cx), marked(cx)), ("Alpha\nBeta漢".into(), None));
    // A dead key: the accent shows, then the letter takes its place.
    compose(cx, None, "^");
    commit(cx, None, "ê");
    assert_eq!(text(cx), "Alpha\nBeta漢ê");
    // Unmarked by the system, what was composed stays as typed.
    compose(cx, None, "ね");
    cx.update(|window, cx| canvas.update(cx, |c, cx| c.unmark_text(window, cx)));
    assert_eq!((text(cx), marked(cx)), ("Alpha\nBeta漢êね".into(), None));
    // A selection is replaced by the composition.
    cx.simulate_keystrokes("shift-left shift-left shift-left");
    compose(cx, None, "に");
    commit(cx, None, "日");
    assert_eq!(text(cx), "Alpha\nBeta日");

    // UTF-16 ranges beside an emoji: two units for one character.
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_input("a😀b");
    assert_eq!((read(cx, 1..3).as_deref(), selection(cx)), (Some("😀"), Some(4..4)));
    commit(cx, Some(3..4), "c");
    compose(cx, Some(1..3), "ç");
    assert_eq!((text(cx), marked(cx)), ("açc".into(), Some(1..2)));
    // Escape in mid-composition ends writing with the text as shown; one undo brings back the label before writing.
    cx.simulate_keystrokes("escape");
    assert_eq!((text(cx), canvas.read_with(cx, |c, _| c.label().is_none())), ("açc".into(), true));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), "Alpha\nBeta");
    cx.simulate_keystrokes("secondary-shift-z");
    assert_eq!(text(cx), "açc");

    // In an arrow's start text, zoomed in and moved: the candidate window still follows the text.
    for _ in 0..4 {
        cx.simulate_input("+");
    }
    let inside = canvas.read_with(cx, |c, _| c.spot(100., 30.));
    cx.simulate_event(gpui::ScrollWheelEvent { position: inside, delta: gpui::ScrollDelta::Pixels(point(px(-53.), px(17.))), modifiers: Modifiers::none(), touch_phase: gpui::TouchPhase::Moved });
    cx.run_until_parked();
    canvas.update(cx, |c, cx| c.write_in(link, Slot::Start, cx));
    cx.run_until_parked();
    compose(cx, None, "ひ");
    let (spot, window) = (canvas.read_with(cx, |c, _| c.spot_of_byte(0).unwrap()), candidates(cx, 0).unwrap());
    assert!((window.origin - spot).x.abs() < px(0.5) && window.origin.y == spot.y, "{window:?} {spot:?}");
    commit(cx, None, "日");
    cx.simulate_keystrokes("escape");
    assert_eq!(canvas.read_with(cx, |c, _| c.diagram().link(link).unwrap().start_text.clone()), "日");
}
