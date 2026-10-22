//! Code blocks, images, pasted images, Mermaid, foreign SVG and the viewer.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let (shell, root) = (&j.shell, &j.root);
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
        // Code block: Enter adds the closing fence; cursor inside, quick copy only
        // takes the block, and the « Copier » button on its opening line does the same.
    cx.simulate_keystrokes("secondary-end enter");
    cx.simulate_input("```rust");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("let x = 1;");
    assert!(text(cx).ends_with("```rust\nlet x = 1;\n```"));
    let clipboard = |cx: &mut gpui::VisualTestContext| cx.read_from_clipboard().and_then(|item| item.text());
    cx.simulate_keystrokes("secondary-shift-c");
    assert_eq!(clipboard(cx).as_deref(), Some("let x = 1;"));
    cx.simulate_keystrokes("secondary-home secondary-shift-c");
    assert!(clipboard(cx).unwrap().starts_with("# Titre"));
    let button = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).copy_button().unwrap());
    cx.simulate_click(button, gpui::Modifiers::none());
    assert_eq!(clipboard(cx).as_deref(), Some("let x = 1;"));

    // An unpaired ``` is the last one, or the one being typed: a cursor resting on a fence of a closed block
    // leaves the pairing alone (the note ends with an unclosed block, the cursor on each fence in turn).
    {
        use markdown::Kind::Code;
        let is_code = |cx: &mut gpui::VisualTestContext, note: &str, probe: &str| j.kind(cx, note.find(probe).unwrap()) == Some(Code);
        let odd = "a\n```\nx\n```\nb\n```\ny\n```\nc\n```\nz";
        let fences: Vec<usize> = odd.match_indices("```").map(|(i, _)| i).collect();
        for cursor in fences.iter().copied().chain([0, odd.len()]) {
            j.load(cx, odd, cursor);
            let seen = [is_code(cx, odd, "x"), is_code(cx, odd, "b"), is_code(cx, odd, "y"), is_code(cx, odd, "c"), is_code(cx, odd, "z")];
            assert_eq!(seen, [true, false, true, false, false], "cursor {cursor}");
        }
        // Even: every fence pairs, wherever the cursor is.
        let even = "```\nx\n```\nb\n```\ny\n```";
        for cursor in even.match_indices("```").map(|(i, _)| i) {
            j.load(cx, even, cursor);
            assert_eq!([is_code(cx, even, "x"), is_code(cx, even, "b"), is_code(cx, even, "y")], [true, false, true], "cursor {cursor}");
        }
        // One fence alone, with or without text after it; indented; in a quote or a list it is not a fence.
        for (note, probe, want) in [("```", "```", false), ("```\nsuite", "suite", false), ("a\n  ```\nsuite", "suite", false), ("> ```\n> x\n> ```\ny", "y", false), ("- ```\n- x", "x", false)] {
            for cursor in [0, note.find("```").unwrap(), note.len()] {
                j.load(cx, note, cursor);
                assert_eq!(is_code(cx, note, probe), want, "{note:?} at {cursor}");
            }
        }
        // Typing a fence in front of a closed block: the new one waits for its partner, the block keeps its code.
        let note = "a\n\n```\nx\n```\nb";
        j.load(cx, note, 2);
        cx.simulate_input("```");
        assert_eq!(text(cx), "a\n```\n```\nx\n```\nb");
        let here = text(cx);
        assert_eq!(j.kind(cx, here.find("x").unwrap()), Some(Code), "the block under the typed fence stays code");
        // Undo, and a note loaded again: no memory of the last edit.
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(text(cx), note);
        let here = text(cx);
        assert_eq!(j.kind(cx, here.find("x").unwrap()), Some(Code));
    }

        // Image: the line that names it carries the image, found next to the note.
        // Symbols (`->`) only display outside the cursor's line, without touching the text.
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20"/></svg>"#;
    fs::write(root.join("carré.svg"), svg).unwrap();
    cx.simulate_keystrokes("secondary-end enter");
    cx.simulate_input("a -> b ![](carré.svg)");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_input("fin");
    cx.run_until_parked();
    let (signs, image) = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).decorations("a -> b"));
    assert_eq!((signs, image), (1, Some((40., 20.))));
    assert!(text(cx).ends_with("a -> b ![](carré.svg)\nfin"));

        // Pasting an image saves it next to the note, which names it.
    let image = gpui::Image::from_bytes(gpui::ImageFormat::Svg, svg.as_bytes().to_vec());
    cx.write_to_clipboard(ClipboardItem::new_image(&image));
    cx.simulate_keystrokes("enter secondary-v");
    let note = text(cx);
    let pasted = note.rsplit_once("![](").unwrap().1.trim_end_matches(')');
    assert!(pasted.starts_with("image-") && pasted.ends_with(".svg"));
    assert_eq!(fs::read_to_string(root.join(pasted)).unwrap(), svg);

        // Mermaid diagram, drawn below its block once the cursor has left, and
        // LaTeX formula, below its line.
    cx.simulate_keystrokes("enter");
    cx.simulate_input("```mermaid");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("flowchart LR");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("A --> B");
    cx.simulate_keystrokes("secondary-end enter");
    cx.simulate_input("soit $x^2$");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_input("fin");
    cx.run_until_parked();
    let figures = shell.read_with(cx, |s, cx| {
        let editor = s.pane().editor.read(cx);
        ["![](image-", "```\nsoit", "soit $x^2$"].map(|line| editor.decorations(line).1.is_some())
    });
    assert_eq!(figures, [true, true, true]);

        // The vault's images are tracked like notes. Those a note displays
        // have their node in the graph; choosing one shows it in place of the note.
    for _ in 0..2 {
        cx.executor().advance_clock(Duration::from_secs(3));
        cx.run_until_parked();
    }
    cx.simulate_keystrokes("secondary-g");
    cx.run_until_parked();
    let (images, notes, nodes) = shell.read_with(cx, |s, cx| {
        (s.images.len(), s.notes.len(), s.graph.read(cx).size().0)
    });
    assert_eq!((images, nodes), (2, notes + 2));
        // An image no note displays also has its node, without a link.
    fs::write(root.join("seule.svg"), svg).unwrap();
    for _ in 0..2 {
        cx.executor().advance_clock(Duration::from_secs(3));
        cx.run_until_parked();
    }
    assert_eq!(shell.read_with(cx, |s, cx| s.graph.read(cx).size().0), notes + 3);
    shell.update(cx, |s, cx| s.preview_note(&root.join("carré.svg"), cx));
    assert_eq!(shell.read_with(cx, |s, _| s.pane().file_path().cloned()), Some(root.join("carré.svg")));
    assert!(text(cx).ends_with("fin"));
        // Going back from an image to the note it hid takes one selection, as
        // from one note to another.
    shell.update(cx, |s, cx| {
        let note = s.pane().path.clone().unwrap();
        s.preview_note(&note, cx);
        assert_eq!(s.pane().file_path(), None);
        s.preview_note(&root.join("carré.svg"), cx);
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| s.pane().file_path().cloned()), None);

        // A foreign SVG (draw.io: HTML text, `light-dark()` colors) shows in the
        // note, drawn in the background. Switching theme, even fast and several times, keeps the
        // figure in place (the old one stays while the new one draws) and accumulates nothing.
    let drawio = r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="120" style="background: light-dark(#fff, #121212)"><rect width="400" height="120" style="fill: light-dark(#ffffff, #121212)"/><switch><foreignObject width="100%" height="100%" requiredFeatures="http://www.w3.org/TR/SVG11/feature#Extensibility"><div xmlns="http://www.w3.org/1999/xhtml" style="display: flex; align-items: unsafe center; justify-content: unsafe center; width: 200px; height: 1px; padding-top: 60px; margin-left: 100px;"><div style="text-align: center;"><div style="display: inline-block; font-size: 12px; color: light-dark(#000, #fff); white-space: normal;"><b>Étape</b><br/>texte</div></div></div></foreignObject><text x="200" y="64">Text is not SVG - cannot display</text></switch></svg>"##;
    fs::write(root.join("drawio.svg"), drawio).unwrap();
    fs::write(root.join("cassé.svg"), "<svg pas fermé").unwrap();
    cx.simulate_keystrokes("secondary-end enter");
    cx.simulate_input("![](drawio.svg)");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("![](cassé.svg)");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_input("fin");
    cx.run_until_parked();
    let seen = |cx: &mut gpui::VisualTestContext| {
        shell.read_with(cx, |s, cx| {
            let editor = s.pane().editor.read(cx);
            (editor.decorations("![](drawio").1, editor.decorations("![](cassé").1, editor.figure_count())
        })
    };
    let (drawn, broken, (count, waiting)) = seen(cx);
    assert_eq!((drawn, broken, waiting), (Some((400., 120.)), None, 0));
    for name in ["Bref Dark", "Bref Light", "Bref Dark", "Bref Light", "Bref Dark"] {
        shell.update_in(cx, |s, window, cx| {
            s.prefs.theme = name.into();
            s.restyle(window, cx);
        });
        cx.simulate_input("x");
        cx.simulate_keystrokes("backspace");
        assert_eq!(seen(cx).0, Some((400., 120.)), "the figure stays in place while it is redrawn");
    }
    cx.run_until_parked();
    cx.simulate_input("x");
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    assert_eq!(seen(cx), (Some((400., 120.)), None, (count, 0)), "no extra figure or drawing");

        // The same SVG, opened, shows in the viewer: fitted to its width, zoomed from the
        // keyboard, and Escape returns to the note. An unreadable SVG says so, without breaking anything.
        // Opened from the tree, it takes input: the note does not keep it.
    shell.update_in(cx, |s, window, cx| {
        s.preview_note(&root.join("drawio.svg"), cx);
        s.focus_pane(0, window, cx);
    });
        // The test platform does not redraw by itself when a background drawing completes.
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..3 {
            cx.run_until_parked();
            cx.update(|window, _| window.refresh());
        }
        cx.run_until_parked();
    };
    settle(cx);
    let viewer = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().viewer().map(|v| v.read(cx).state()));
    let (loaded, failed, fit, _) = viewer(cx).unwrap();
    assert!(loaded && !failed && fit > 0. && fit <= 1., "{:?}", viewer(cx));
    cx.simulate_keystrokes("secondary-=");
    assert!((viewer(cx).unwrap().2 - fit * 1.25).abs() < 1e-4);
    cx.simulate_keystrokes("secondary-0");
    settle(cx);
    assert_eq!(viewer(cx).unwrap().2, fit);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.pane().file.is_none() && s.pane().viewer().is_none()));
        // A Mermaid block's magnifier shows the diagram large, in place of the note; + and -
        // zoom; Escape returns to the note.
    let button = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).view_button("```mermaid")).unwrap();
    cx.simulate_click(button, gpui::Modifiers::none());
    settle(cx);
    let figure = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().figure.as_ref().map(|f| f.read(cx).state()));
    let (loaded, failed, fit, _) = figure(cx).unwrap();
    assert!(loaded && !failed && fit > 0., "{:?}", figure(cx));
    assert!(text(cx).contains("```mermaid\nflowchart LR"), "the note did not move");
    cx.simulate_keystrokes("+");
    assert!((figure(cx).unwrap().2 - fit * 1.25).abs() < 1e-4);
    cx.simulate_keystrokes("- 0");
    settle(cx);
    assert_eq!(figure(cx).unwrap().2, fit);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.pane().figure.is_none()));
    shell.update_in(cx, |s, window, cx| {
        s.preview_note(&root.join("cassé.svg"), cx);
        s.focus_pane(0, window, cx);
    });
    settle(cx);
    assert_eq!(viewer(cx).map(|v| (v.0, v.1)), Some((false, true)));
    shell.update(cx, |s, cx| {
        let note = s.pane().path.clone().unwrap();
        s.preview_note(&note, cx);
    });
    cx.run_until_parked();
}
