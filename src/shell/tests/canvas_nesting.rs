//! A frame drawn around a shape and an arrow: what each click, double click, right click and pointer takes
//! there, and what the saved file keeps.

use super::*;
use gpui::{Modifiers, MouseButton};

#[gpui::test]
fn what_a_frame_surrounds_stays_reachable(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "canvas-nesting");
    let file = root.join("Nested.svg");
    let mut diagram = Diagram::blank();
    let inner = diagram.add_shape(diagram::Form::Rect, 200., 200., 200., 100.);
    diagram.shape_mut(inner).unwrap().text = "Inner".into();
    let link = diagram.add_link(diagram::End::Point(50., 500.), diagram::End::Point(750., 500.), diagram::Head::Arrow, diagram::Route::Straight);
    fs::write(&file, diagram.to_svg(diagram::COLORS[0])).unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    let canvas = shell.update_in(cx, |s, window, cx| {
        s.load_note(&file, cx);
        let canvas = s.pane().drawing().cloned().unwrap();
        window.focus(&canvas.focus_handle(cx));
        canvas
    });
    cx.run_until_parked();
    let spot = |cx: &mut gpui::VisualTestContext, p: (f32, f32)| canvas.read_with(cx, |c, _| c.spot(p.0, p.1));
    let saved = || Diagram::from_svg(&fs::read_to_string(&file).unwrap()).unwrap();
    let drag = |cx: &mut gpui::VisualTestContext, from: (f32, f32), to: (f32, f32)| {
        let (from, to) = (spot(cx, from), spot(cx, to));
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    };

    // A frame drawn around them, after them: it takes in the shape wholly inside and draws it over itself.
    cx.simulate_input("f");
    drag(cx, (0., 0.), (800., 600.));
    let drawn = saved();
    let frame = drawn.shapes.iter().find(|s| s.form == diagram::Form::Frame).unwrap().id;
    assert_eq!(drawn.shape(inner).unwrap().parent, Some(frame));
    assert_eq!(drawn.shapes.iter().map(|s| s.id).collect::<Vec<_>>(), vec![frame, inner]);

    let click = |cx: &mut gpui::VisualTestContext, p: (f32, f32), held: Modifiers| {
        let at = spot(cx, p);
        cx.simulate_click(at, held);
        canvas.read_with(cx, |c, _| c.selected_ids())
    };
    // A click: the shape inside, the arrow across, the frame on its border and in its empty inside.
    assert_eq!(click(cx, (300., 250.), Modifiers::none()), vec![inner]);
    assert_eq!(click(cx, (400., 500.), Modifiers::none()), vec![link]);
    assert_eq!(click(cx, (0., 300.), Modifiers::none()), vec![frame]);
    assert_eq!(click(cx, (600., 100.), Modifiers::none()), vec![frame]);
    // Alt+click goes through what is under the pointer, then round to the top.
    assert_eq!(click(cx, (300., 250.), Modifiers::none()), vec![inner]);
    assert_eq!(click(cx, (300., 250.), Modifiers::alt()), vec![frame]);
    assert_eq!(click(cx, (300., 250.), Modifiers::alt()), vec![inner]);
    assert_eq!(click(cx, (400., 500.), Modifiers::alt()), vec![link]);
    // Shift+click adds to the selection, through the frame.
    click(cx, (600., 100.), Modifiers::none());
    assert_eq!(click(cx, (300., 250.), Modifiers::shift()), vec![frame, inner]);

    // The pointer lights what a click would take.
    for (p, expected) in [((300., 250.), inner), ((400., 500.), link), ((600., 100.), frame)] {
        let at = spot(cx, p);
        cx.simulate_mouse_move(at, None, Modifiers::none());
        assert_eq!(canvas.read_with(cx, |c, _| c.pointed().0), Some(expected), "{p:?}");
    }

    // A double click writes in the shape inside, not in the frame.
    let at = spot(cx, (300., 250.));
    cx.simulate_click(at, Modifiers::none());
    cx.simulate_event(gpui::MouseDownEvent { position: at, modifiers: Modifiers::none(), button: MouseButton::Left, click_count: 2, first_mouse: false });
    cx.simulate_event(gpui::MouseUpEvent { position: at, modifiers: Modifiers::none(), button: MouseButton::Left, click_count: 2 });
    assert_eq!(canvas.read_with(cx, |c, _| c.writing()), Some(inner));
    cx.simulate_keystrokes("escape");

    // A right click selects what is under it and opens its menu.
    let at = spot(cx, (400., 500.));
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
    assert_eq!(canvas.read_with(cx, |c, _| (c.selected_ids(), c.pointed().1)), (vec![link], true));
    cx.simulate_keystrokes("escape");

    // An arrow drawn to the shape inside attaches to it, not to the frame over it.
    cx.simulate_input("a");
    drag(cx, (600., 250.), (300., 250.));
    let last = saved().links.last().unwrap().clone();
    assert!(matches!(last.to, diagram::End::Shape(id) | diagram::End::Pin(id, ..) if id == inner), "{:?}", last.to);

    // Moving the frame takes what it holds along; read again, the file keeps the holder and the order.
    cx.simulate_keystrokes("escape");
    click(cx, (0., 300.), Modifiers::none());
    drag(cx, (0., 300.), (100., 300.));
    let drawn = saved();
    assert_eq!((drawn.shape(frame).unwrap().x, drawn.shape(inner).unwrap().x, drawn.shape(inner).unwrap().parent), (100., 300., Some(frame)));
    assert_eq!(drawn.hit(400., 250., 7.), Some(inner));
}
