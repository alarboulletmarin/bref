//! Headings, then code blocks, callouts and quotes: where each one starts and ends, the room inside its fill, and the
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

/// The six heading levels: a scale, room above (none on the first line), `#` markers that are no title when there
/// are seven or no space, and a caret and clicks that agree on the marker and on the title.
pub(super) fn headings(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let note = "# un\n\n## deux\n### trois\n#### quatre\n##### cinq\n###### six\n####### sept\n#tag\n## \n# Éa\n";
    j.load(cx, note, 0);
    let rows = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).heading_rows());
    let at = |s: &str| note.find(s).unwrap();
    // `#######` and `#tag` are no title; an empty `## ` and a one letter title are.
    let found: Vec<(usize, u8)> = rows.iter().map(|r| (r.0, r.1)).collect();
    assert_eq!(found, [(0, 1), (at("## deux"), 2), (at("### trois"), 3), (at("#### quatre"), 4), (at("##### cinq"), 5), (at("###### six"), 6), (at("## \n"), 2), (at("# Éa"), 1)]);
    let lh = |n: u8| rows.iter().find(|r| r.1 == n).unwrap().3;
    // Line heights fall with the level (H5 and H6 share the body size), room above: none on the first line, then a
    // body line for H1 and H2, two thirds of it for the rest.
    assert!(lh(1) > lh(2) && lh(2) > lh(3) && lh(3) > lh(4) && lh(4) > lh(5) && lh(5) == lh(6), "{rows:?}");
    let pad = |n: u8| rows.iter().filter(|r| r.1 == n).last().unwrap().2;
    assert_eq!(rows[0].2, 0.);
    assert!(pad(1) == pad(2) && pad(2) > pad(3) && pad(3) == pad(6) && pad(6) > 0., "{rows:?}");
    // Caret and click land on the same byte: on the marker and on the first letter of each title.
    shell.update_in(cx, |s, window, cx| window.focus(&s.pane().editor.focus_handle(cx)));
    cx.run_until_parked();
    for target in ["# un", "un", "## deux", "deux", "### trois", "trois", "#### quatre", "quatre", "##### cinq", "cinq", "###### six", "six", "Éa"] {
        let start = if target.starts_with('#') { at(target) } else { note[..].rfind(target).unwrap() };
        let p = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).point_of(start).unwrap());
        cx.simulate_click(p + point(px(1.), px(1.)), gpui::Modifiers::none());
        let caret = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).position());
        assert_eq!(caret, start, "click on {target:?}");
    }
}

/// Tags are pills painted behind the text: a note with tags lays out exactly as the same note with the tags turned
/// into plain words, so the caret, the selection and the clicks stay where they were. A tag cut by a wrap (a long one,
/// one at the end of a wrapped line), one in a table cell, one in a callout and in a list are measured too.
pub(super) fn tags(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let long = format!("#{}", "mot".repeat(60));
    let note = format!(
        "Un #tag et #deux/trois, {long} puis la suite {} #fin\n\n- liste #item\n\n> [!TIP]\n> conseil #astuce\n\n| a | b #c |\n|---|---|\n| #d | e |\n\n`#code` et ##x\n",
        "assez longue pour passer à la ligne ".repeat(4)
    );
    // The same text, every tag's `#` replaced by a letter of the same width: no tag, so no pill.
    let plain = note.replace(" #", " x").replace("| #", "| x").replace("\n\n#", "\n\nx");
    assert_eq!(plain.len(), note.len());
    let geometry = |cx: &mut gpui::VisualTestContext, text: &str| {
        j.load(cx, text, 0);
        shell.update_in(cx, |s, window, cx| window.focus(&s.pane().editor.focus_handle(cx)));
        cx.run_until_parked();
        shell.read_with(cx, |s, cx| {
            let editor = s.pane().editor.read(cx);
            (0..=text.len()).filter(|i| text.is_char_boundary(*i)).map(|i| editor.point_of(i)).collect::<Vec<_>>()
        })
    };
    let (with, without) = (geometry(cx, &note), geometry(cx, &plain));
    // Rows are as tall and the text is where it was, down to the byte (no layout change, so none to redo either).
    let pairs = with.iter().zip(&without).filter(|(a, b)| a != b).count();
    assert_eq!(pairs, 0, "{pairs} byte positions moved because of tags");
    // The long tag wraps (its two halves are on two lines), and a click on it still lands on the byte.
    let at = note.find(&long).unwrap();
    let (first, last) = (with[at].unwrap(), with[at + long.len() - 1].unwrap());
    assert!(last.y > first.y, "the long tag is not wrapped: {first:?} {last:?}");
    j.load(cx, &note, 0);
    shell.update_in(cx, |s, window, cx| window.focus(&s.pane().editor.focus_handle(cx)));
    cx.run_until_parked();
    for target in [note.find("#tag").unwrap(), note.find("#fin").unwrap() + 2, at + 90, note.find("#item").unwrap() + 1, note.find("#astuce").unwrap()] {
        let p = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).point_of(target).unwrap());
        cx.simulate_click(p + point(px(1.), px(0.)), gpui::Modifiers::none());
        let caret = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).position());
        assert_eq!(caret, target, "click on byte {target}");
    }
}
