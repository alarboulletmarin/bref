//! Code blocks, callouts and quotes: where each one starts and ends, the room inside its fill, and the
//! caret and the clicks on its first and last lines.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let lines = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).block_lines());
    let of = |lines: &[(usize, bool, bool, f32, f32)], at: usize| *lines.iter().find(|l| l.0 == at).unwrap();
    // Block alone, one line, two, a callout followed straight by a code block, two callouts in a row, a plain quote
    // (a bar, no room), a ``` with no closing one (a block of one line).
    let note = "```rust\nlet a = 1;\n```\n\n> [!NOTE]\n> une\n> deux\n```\nx\n```\n> [!TIP]\n> seule\n> [!WARNING]\n> autre\n\n> simple\n\n```\ndeux\n```\n\n```\n";
    j.load(cx, note, 0);
    let l = lines(cx);
    let at = |s: &str| note.find(s).unwrap();
    let room = l[0].3;
    assert!(room > 0., "{l:?}");
    // Code: the room above the opening line and under the closing one only.
    assert_eq!(of(&l, 0), (0, true, false, room, 0.));
    assert_eq!(of(&l, at("let")), (at("let"), false, false, 0., 0.));
    assert_eq!(of(&l, at("```\n\n> [!NOTE]")), (at("```\n\n> [!NOTE]"), false, true, 0., room));
    // Callout: from its `[!TYPE]` line to the line before the next block, a gap between it and the code block.
    assert_eq!(of(&l, at("> [!NOTE]")), (at("> [!NOTE]"), true, false, room, 0.));
    assert_eq!(of(&l, at("> une")), (at("> une"), false, false, 0., 0.));
    assert_eq!(of(&l, at("> deux")), (at("> deux"), false, true, 0., room));
    assert_eq!(of(&l, at("```\nx")).1, true);
    // Two callouts in a row are two shapes; a callout of a single line has both ends.
    assert_eq!(of(&l, at("> seule")), (at("> seule"), false, true, 0., room));
    assert_eq!(of(&l, at("> [!WARNING]")), (at("> [!WARNING]"), true, false, room, 0.));
    // A plain quote has the ends of its bar but no room.
    assert_eq!(of(&l, at("> simple")), (at("> simple"), true, true, 0., 0.));
    // A ``` that closes nothing: one line.
    let orphan = note.rfind("```").unwrap();
    assert_eq!(of(&l, orphan), (orphan, true, true, room, room));
    // A block at the very top and a block at the very bottom of the note, and a single line block.
    for note in ["```\nx\n```", "```\n```\n", "> [!NOTE]", "> [!NOTE]\n> a", "> a\n> b\n> c"] {
        j.load(cx, note, 0);
        let l = lines(cx);
        assert!(l.first().unwrap().1 && l.last().unwrap().2, "{note:?}: {l:?}");
        assert_eq!(l.iter().filter(|l| l.1).count(), 1, "{note:?}: {l:?}");
        assert_eq!(l.iter().filter(|l| l.2).count(), 1, "{note:?}: {l:?}");
    }
    // The room moves the text down, not the clicks: the caret and a click land on the same byte on the first and last
    // lines of a fence and of a callout, whether the cursor is inside the block or not.
    let note = "avant\n\n```rust\nfin\n```\n\n> [!NOTE]\n> fin\n\napres\n";
    for inside in [false, true] {
        j.load(cx, note, if inside { note.find("fin").unwrap() } else { 0 });
        shell.update_in(cx, |s, window, cx| window.focus(&s.pane().editor.focus_handle(cx)));
        cx.run_until_parked();
        for target in [note.find("rust").unwrap(), note.find("```\n\n> ").unwrap(), note.find("NOTE").unwrap(), note.rfind("fin").unwrap()] {
            let p = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).point_of(target).unwrap());
            cx.simulate_click(p + point(px(1.), px(0.)), gpui::Modifiers::none());
            let caret = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).position());
            assert_eq!(caret, target, "inside {inside}, click on byte {target}");
        }
    }
}
