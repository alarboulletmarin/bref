//! Drawing is a pure function of the state: a frame without an event changes nothing, and
//! every way of showing a note enters the history exactly once.

use super::*;
use crate::logic::history::Shown;

/// What a frame could have changed: the history, the panel, the window chrome of the note.
fn state(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext) -> (Vec<Shown>, usize, String, editor::Chrome) {
    shell.read_with(cx, |s, cx| {
        let (list, at) = s.history.trail();
        (list, at, format!("{:?} {:?}", s.nav.panel(), s.active), s.pane().editor.read(cx).chrome())
    })
}

fn frame(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

#[gpui::test]
fn a_frame_without_event_changes_nothing_and_each_way_in_is_recorded_once(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "render-pure");
    let note = |name: &str| root.join(format!("{name}.md"));
    for name in ["Alpha", "Beta", "Gamma", "Delta"] {
        fs::write(note(name), format!("# {name}\n\n[[Delta]]\n")).unwrap();
    }
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    let entries = |cx: &mut gpui::VisualTestContext| state(&shell, cx).0.len();
    let was = |cx: &mut gpui::VisualTestContext, path: PathBuf| assert_eq!(state(&shell, cx).0.last(), Some(&Shown::Note(path)));

    // Nothing shown yet, then a note opened from the panel.
    frame(cx);
    assert_eq!(entries(cx), 0);
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&note("Alpha"), window, cx));
    frame(cx);
    assert_eq!(entries(cx), 1);
    was(cx, note("Alpha"));

    // Frames alone: the same state, three times.
    let before = state(&shell, cx);
    for _ in 0..3 {
        frame(cx);
        assert!(state(&shell, cx) == before, "a frame changed the state");
    }

    // The palette.
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("beta");
    cx.simulate_keystrokes("enter");
    frame(cx);
    assert_eq!(entries(cx), 2);
    was(cx, note("Beta"));

    // A link followed.
    shell.update_in(cx, |s, window, cx| s.follow(&Link::Wiki("Delta".into()), window, cx));
    frame(cx);
    assert_eq!(entries(cx), 3);
    was(cx, note("Delta"));

    // A tab brought back to the front.
    cx.simulate_keystrokes("ctrl-tab");
    frame(cx);
    assert_eq!(entries(cx), 4);

    // Frames again, after all of it.
    let before = state(&shell, cx);
    frame(cx);
    frame(cx);
    assert!(state(&shell, cx) == before, "a frame changed the state");

    // Restored at start: the panes come back, the shown tab enters the history once.
    shell.update_in(cx, |s, window, cx| s.restore_panes(None, vec![(vec![note("Gamma"), note("Alpha")], 0)], true, &root, window, cx));
    frame(cx);
    frame(cx);
    assert_eq!(entries(cx), 5);
    was(cx, note("Gamma"));
}
