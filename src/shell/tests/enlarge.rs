//! The "enlarge" button that appears when the pointer is over a picture, a formula or a figure
//! of the note: where it sits, what it opens, how the viewer then behaves.

use super::*;
use gpui::{Bounds, Modifiers, Pixels, ScrollDelta, ScrollWheelEvent, TouchPhase, point, px, size};

/// A grayscale PNG of `w` x `h` pixels (stored blocks: no compression library needed).
fn png(w: u32, h: u32) -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut c = !0u32;
        for &b in bytes {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 == 1 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    let chunk = |kind: &[u8], data: &[u8]| {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        let body = [kind, data].concat();
        out.extend(&body);
        out.extend(crc(&body).to_be_bytes());
        out
    };
    let raw: Vec<u8> = (0..h).flat_map(|_| std::iter::once(0).chain((0..w).map(|x| (x * 7 % 200) as u8 + 20))).collect();
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push((i + 1 == blocks.len()) as u8);
        z.extend((block.len() as u16).to_le_bytes());
        z.extend((!(block.len() as u16)).to_le_bytes());
        z.extend(*block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &v in &raw {
        a = (a + v as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend(((b << 16) | a).to_be_bytes());
    let mut head = w.to_be_bytes().to_vec();
    head.extend(h.to_be_bytes());
    head.extend([8, 0, 0, 0, 0]);
    [b"\x89PNG\r\n\x1a\n".to_vec(), chunk(b"IHDR", &head), chunk(b"IDAT", &z), chunk(b"IEND", &[])].concat()
}

const GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";

fn inside(inner: Bounds<Pixels>, outer: Bounds<Pixels>) -> bool {
    inner.left() >= outer.left() && inner.right() <= outer.right() && inner.top() >= outer.top() && inner.bottom() <= outer.bottom()
}

type Found = Vec<(usize, Bounds<Pixels>, Bounds<Pixels>, bool)>;

/// The test window with a vault holding the pictures, and what the tests do with it.
struct Page {
    shell: Entity<Shell>,
}

impl Page {
    fn load(&self, cx: &mut gpui::VisualTestContext, text: &str) {
        let len = text.len();
        self.shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.load(text.to_string(), len, cx)));
        self.shell.update_in(cx, |s, window, cx| window.focus(&s.pane().editor.focus_handle(cx)));
        self.settle(cx);
    }

    fn settle(&self, cx: &mut gpui::VisualTestContext) {
        for _ in 0..4 {
            cx.run_until_parked();
            cx.update(|window, _| window.refresh());
        }
        cx.run_until_parked();
    }

    fn found(&self, cx: &mut gpui::VisualTestContext) -> Found {
        self.shell.read_with(cx, |s, cx| s.pane().editor.read(cx).enlargeable())
    }

    fn hover(&self, cx: &mut gpui::VisualTestContext, at: gpui::Point<Pixels>) {
        cx.simulate_mouse_move(at, None, Modifiers::none());
        cx.run_until_parked();
    }

    fn viewer(&self, cx: &mut gpui::VisualTestContext) -> Option<(bool, bool, f32, (f32, f32))> {
        self.shell.read_with(cx, |s, cx| s.pane().figure.as_ref().map(|f| f.read(cx).state()))
    }

    fn wheel(&self, cx: &mut gpui::VisualTestContext, at: gpui::Point<Pixels>, dy: f32) {
        cx.simulate_event(ScrollWheelEvent {
            position: at,
            delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
            modifiers: Modifiers::secondary_key(),
            touch_phase: TouchPhase::Moved,
        });
        cx.run_until_parked();
    }
}

#[gpui::test]
fn picture_formula_and_figure_enlarge_from_a_button(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-enlarge-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    for (name, bytes) in [
        ("un.png", png(1, 1)),
        ("grand.png", png(300, 200)),
        ("large.png", png(8000, 20)),
        ("haut.png", png(60, 3000)),
        ("fin.gif", GIF.to_vec()),
        ("cassée.png", b"pas une image".to_vec()),
    ] {
        fs::write(root.join(name), bytes).unwrap();
    }
    fs::write(root.join("schéma.svg"), r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect width="200" height="100" fill="#369"/></svg>"##).unwrap();
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
    cx.run_until_parked();
    let page = Page { shell };
    let window = |cx: &mut gpui::VisualTestContext| Bounds::new(point(px(0.), px(0.)), cx.update(|w, _| w.viewport_size()));

    // Every kind, at every width: the button sits in the picture's top right corner when the
    // picture can hold it, beside it when not, inside the window and its band, and does not move
    // between hovering the picture and hovering the button.
    let cases = [
        ("![](un.png)", "1x1"),
        ("![](grand.png)", "300x200"),
        ("![](large.png)", "8000x20, scaled to the page"),
        ("![](haut.png)", "60x3000"),
        ("![](fin.gif)", "gif"),
        ("![](schéma.svg)", "svg of the note"),
        ("$$\nx^2 + y^2\n$$", "display formula"),
        ("soit $x^2$ ici", "inline formula"),
        ("> ![](grand.png)", "picture in a quote"),
        ("- ![](grand.png)", "picture in a list"),
    ];
    for width in [420., 1100., 1700.] {
        cx.simulate_resize(size(px(width), px(900.)));
        for (line, what) in cases {
            page.load(cx, &format!("avant\n\n{line}\n\napres\n\nfin"));
            let found = page.found(cx);
            assert_eq!(found.len(), 1, "{what} at {width}: {found:?}");
            let (_, pic, btn, _) = found[0];
            assert_eq!(btn.size, size(px(22.), px(22.)), "{what} at {width}");
            assert!(inside(btn, window(cx)), "{what} at {width}: {btn:?} outside the window");
            let band = (pic.top(), pic.top() + pic.size.height.max(px(22.)));
            assert!(btn.top() >= band.0 && btn.bottom() <= band.1, "{what} at {width}: {btn:?} leaves the band of {pic:?}");
            if pic.size.width >= px(44.) && pic.size.height >= px(44.) && !line.contains('$') {
                // Top right corner; a picture scrolled up keeps the button in view.
                assert!(inside(btn, pic) && pic.right() - btn.right() == px(6.), "{what} at {width}: {btn:?} on {pic:?}");
                assert!(pic.size.height > px(900.) || btn.top() - pic.top() == px(6.), "{what} at {width}: corner");
            } else if btn.left() < pic.right() {
                // A full line: the button is at its right edge, over the picture.
                assert_eq!(btn.right(), pic.right(), "{what} at {width}");
            } else {
                assert!(btn.left() >= pic.right(), "{what} at {width}: {btn:?} overlaps {pic:?}");
            }
            page.hover(cx, point(px(width - 10.), btn.center().y));
            assert!(!page.found(cx)[0].3, "{what}: shown with the pointer away");
            page.hover(cx, point(pic.left() + pic.size.width / 2., btn.center().y.clamp(pic.top(), pic.bottom() - px(0.5))));
            let over = page.found(cx)[0];
            assert!(over.3 && over.2 == btn, "{what} at {width}: shown over the picture, same place: {over:?}");
            page.hover(cx, btn.center());
            let on = page.found(cx)[0];
            assert!(on.3 && on.2 == btn, "{what} at {width}: still shown, same place, over the button");
        }
    }

    // Missing, unreadable and remote pictures: no button, no panic.
    cx.simulate_resize(size(px(1100.), px(900.)));
    page.load(cx, "![](absente.png)\n\n![](cassée.png)\n\n![](https://exemple.fr/a.png)\n\nfin");
    assert!(page.found(cx).is_empty());

    // Two pictures in a row: each button opens its own picture.
    page.load(cx, "![](grand.png)\n![](schéma.svg)\nfin");
    let found = page.found(cx);
    assert_eq!(found.len(), 2);
    assert!(found[0].1.bottom() <= found[1].1.top() && found[0].2.bottom() <= found[1].1.top() + px(1.), "{found:?}");
    page.hover(cx, found[1].1.origin + point(px(5.), px(5.)));
    let now = page.found(cx);
    assert_eq!((now[0].3, now[1].3), (false, true), "only the picture under the pointer shows its button");
    cx.simulate_click(found[1].2.center(), Modifiers::none());
    page.settle(cx);
    let svg = page.viewer(cx).expect("the second picture opens");
    assert!(svg.0 && !svg.1, "{svg:?}: the SVG is read");
    cx.simulate_keystrokes("escape");
    assert!(page.viewer(cx).is_none());

    // A click on the picture itself does not open anything.
    page.load(cx, "![](grand.png)\nfin");
    let found = page.found(cx);
    cx.simulate_click(found[0].1.center(), Modifiers::none());
    page.settle(cx);
    assert!(page.viewer(cx).is_none());

    // Raster viewer: opens fitted, zoom keys and bounds, wheel toward the pointer, drag, Escape.
    for (line, what) in [("![](grand.png)", "300x200"), ("![](un.png)", "1x1"), ("![](large.png)", "8000x20"), ("![](haut.png)", "60x3000"), ("![](fin.gif)", "gif")] {
        page.load(cx, &format!("{line}\nfin"));
        let found = page.found(cx);
        page.hover(cx, found[0].1.origin + point(px(5.), px(5.)));
        cx.simulate_click(found[0].2.center(), Modifiers::none());
        page.settle(cx);
        let (loaded, failed, fit, _) = page.viewer(cx).unwrap_or_else(|| panic!("{what}: not opened"));
        assert!(loaded && !failed && fit > 0. && fit <= 1., "{what}: {:?}", page.viewer(cx));
        cx.simulate_keystrokes("+");
        assert!((page.viewer(cx).unwrap().2 - fit * 1.25).abs() < 1e-4, "{what}");
        for _ in 0..40 {
            cx.simulate_keystrokes("-");
        }
        assert_eq!(page.viewer(cx).unwrap().2, 0.05, "{what}: minimum zoom");
        for _ in 0..40 {
            cx.simulate_keystrokes("+");
        }
        assert_eq!(page.viewer(cx).unwrap().2, 8., "{what}: maximum zoom");
        cx.simulate_keystrokes("0");
        page.settle(cx);
        assert_eq!(page.viewer(cx).unwrap().2, fit, "{what}: fit");
        // The wheel with Ctrl zooms around the pointer: the point under it stays there.
        let (zoom, pan) = (page.viewer(cx).unwrap().2, page.viewer(cx).unwrap().3);
        let at = point(px(500.), px(300.));
        page.wheel(cx, at, 60.);
        let (zoom2, pan2) = (page.viewer(cx).unwrap().2, page.viewer(cx).unwrap().3);
        assert!(zoom2 > zoom, "{what}: the wheel zooms in");
        let origin = page.shell.read_with(cx, |s, cx| s.pane().figure.as_ref().unwrap().read(cx).origin());
        let m = (500. - f32::from(origin.x), 300. - f32::from(origin.y));
        let under = |zoom: f32, pan: (f32, f32)| ((m.0 - pan.0) / zoom, (m.1 - pan.1) / zoom);
        let (u1, u2) = (under(zoom, pan), under(zoom2, pan2));
        assert!((u1.0 - u2.0).abs() < 0.5 && (u1.1 - u2.1).abs() < 0.5, "{what}: {u1:?} {u2:?}");
        // Drag moves the picture.
        cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(at + point(px(40.), px(25.)), MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(at + point(px(40.), px(25.)), MouseButton::Left, Modifiers::none());
        let pan3 = page.viewer(cx).unwrap().3;
        assert_eq!((pan3.0 - pan2.0, pan3.1 - pan2.1), (40., 25.), "{what}: drag");
        // Escape in the middle of a zoom: back to the note, which did not move.
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(page.viewer(cx).is_none(), "{what}: Escape");
        assert!(page.shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().starts_with(line)));
    }

    // Formulas and a note's SVG open in the SVG viewer.
    for (line, what) in [("$$\nx^2 + y^2\n$$", "display formula"), ("soit $x^2$ ici", "inline formula"), ("![](schéma.svg)", "svg")] {
        for theme in ["Bref Light", "Bref Dark"] {
            page.shell.update_in(cx, |s, window, cx| {
                s.prefs.theme = theme.into();
                s.restyle(window, cx);
            });
            page.load(cx, &format!("{line}\nfin"));
            let found = page.found(cx);
            cx.simulate_click(found[0].2.center(), Modifiers::none());
            page.settle(cx);
            let (loaded, failed, ..) = page.viewer(cx).unwrap_or_else(|| panic!("{what}: not opened"));
            assert!(loaded && !failed, "{what} ({theme})");
            cx.simulate_keystrokes("escape");
            assert!(page.viewer(cx).is_none());
        }
    }

    // A table cell holding a picture: whatever the grid decides, no panic and no stray button.
    page.load(cx, "| a | b |\n| --- | --- |\n| ![](grand.png) | x |\n\nfin");
    assert!(page.found(cx).iter().all(|(_, pic, btn, _)| inside(*btn, window(cx)) && pic.size.width > px(0.)));

    // A viewer on a missing or unreadable picture says so, and Escape still returns to the note.
    for name in ["absente.png", "cassée.png"] {
        page.load(cx, "texte\nfin");
        let theme = page.shell.read_with(cx, |s, _| s.theme);
        let source = crate::views::viewer::Source::Raster(root.join(name));
        let figure = cx.update(|window, cx| {
            let figure = cx.new(|cx| crate::views::viewer::Viewer::new(source, theme, cx));
            window.focus(&figure.focus_handle(cx));
            figure
        });
        page.shell.update(cx, |s, cx| {
            s.pane_mut().figure = Some(figure);
            cx.notify();
        });
        page.settle(cx);
        let (loaded, failed, ..) = page.viewer(cx).unwrap();
        assert!(!loaded && failed, "{name}");
        cx.simulate_keystrokes("escape");
        assert!(page.viewer(cx).is_none(), "{name}");
    }
    let _ = fs::remove_dir_all(&root);
}

/// Between an edit and the next paint the lines still hold the old text's offsets: a pointer move or
/// a click there used to slice the new text with them and panic (Windows: the process aborted).
#[gpui::test]
fn pointer_events_between_an_edit_and_the_layout_do_not_panic(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-stale-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
    cx.run_until_parked();
    let page = Page { shell };
    let long = "- [ ] première tâche\n".to_string() + &"```mermaid\ngraph TD\nA-->B\n```\n\n".repeat(12) + "- [ ] tâche\n";
    // The shorter text cuts nothing, falls inside no character, or leaves a lone multi-byte one.
    for short in ["", "x", "é", "日本", "```mermaid"] {
        page.load(cx, &long);
        // In one go: no paint between the edit and the events.
        page.shell.update_in(cx, |s, window, cx| {
            s.pane().editor.update(cx, |e, cx| {
                e.load(short.to_string(), short.len(), cx);
                e.pointer(window, cx);
            })
        });
        page.settle(cx);
    }
    let _ = fs::remove_dir_all(&root);
}
