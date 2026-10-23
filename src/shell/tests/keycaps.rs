//! Shortcuts drawn as one badge per key: the shared widget by itself, then on the surfaces that
//! show a shortcut (help, context menu, palette, empty page), measured.

use super::*;
use crate::ui::widgets::keycaps;
use gpui::{Bounds, Context, IntoElement, Render, Window, div};

/// Each shortcut in a row of its own, in a box of the given width.
struct Probe {
    width: Pixels,
    on: bool,
}

const CASES: [&str; 8] = ["", "A", "Ctrl+K", "Ctrl+Maj+C", "Ctrl++", "+", "Ctrl+Alt+Maj+Suppr", "F10"];

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (width, on) = (self.width, self.on);
        let t = Theme::of(&Prefs::parse(""), gpui::WindowAppearance::Light);
        div().w(width).flex().flex_col().items_start().gap_2().children(CASES.iter().enumerate().map(move |(i, keys)| {
            keycaps(keys, t, on).flex_wrap().debug_selector(move || format!("probe-{i}"))
        }))
    }
}


fn bounds_of(cx: &mut gpui::VisualTestContext, name: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(name).unwrap_or_else(|| panic!("{name} absent"))
}

#[gpui::test]
fn keycaps_widget(cx: &mut TestAppContext) {
    cx.update(init_fonts);
    for (width, on) in [(px(400.), false), (px(400.), true), (px(60.), false)] {
        let (_, cx) = cx.add_window_view(|_, _| Probe { width, on });
        cx.run_until_parked();
        // No keys: nothing is drawn.
        let empty = cx.debug_bounds("probe-0").map(|b| b.size);
        assert!(empty.is_none_or(|s| s.height == px(0.)), "{empty:?}");
        let one = bounds_of(cx, "probe-1");
        let [two, three, plus_chord, plus, long, f10] = [2, 3, 4, 5, 6, 7].map(|i| bounds_of(cx, leak(format!("probe-{i}"))));
        // Too narrow for the badges side by side: they wrap, and none leaves the box.
        if width < px(100.) {
            for b in [two, three, plus_chord, long] {
                assert!(b.size.width <= width && b.size.height > one.size.height, "{b:?}");
            }
            continue;
        }
        // One line of badges as high as a single one, as wide as the keys need.
        assert!(one.size.height > px(10.) && one.size.width >= px(18.));
        assert!((two.size.height - one.size.height).abs() < px(1.), "{two:?} {one:?}");
        assert!((three.size.height - one.size.height).abs() < px(1.), "{three:?} {one:?}");
        assert!(two.size.width > one.size.width && three.size.width > two.size.width);
        // "+" alone, and "Ctrl++" (Ctrl and +), are badges, not an empty or a broken row.
        assert!((plus.size.width - one.size.width).abs() < px(2.), "{plus:?} {one:?}");
        assert!(plus_chord.size.width > plus.size.width && (plus_chord.size.height - one.size.height).abs() < px(1.));
        assert!(f10.size.width >= px(20.) && (f10.size.height - one.size.height).abs() < px(1.));
        assert!((long.size.height - one.size.height).abs() < px(1.), "{long:?}");
        assert!(long.size.width <= width);
        // Selected or not, the badges are the same size.
        assert_eq!(three.size.width, bounds_of(cx, "probe-3").size.width);
    }
}

/// The help panel: the button that edits a shortcut shows badges and stays in its 156 px
/// column, whichever line; a shortcut being typed or removed keeps the same button.
#[gpui::test]
fn keycaps_in_help(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-keycaps-help-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (_shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
    cx.run_until_parked();
    for (width, height) in [(1400., 900.), (860., 720.), (420., 720.)] {
        cx.simulate_resize(size(px(width), px(height)));
        cx.simulate_keystrokes("f1");
        cx.run_until_parked();
        let mut seen = 0;
        for (_, rows) in help_sections(&[], &keys::Changes::new()) {
            for row in rows {
                let Some(action) = row.action else { continue };
                let (Some(column), Some(button)) = (cx.debug_bounds(leak(format!("key-column-{action}"))), cx.debug_bounds(leak(format!("key-{action}")))) else { continue };
                seen += 1;
                assert!(column.size.width <= px(156.5), "{action}: {column:?}");
                assert!(button.left() >= column.left() && button.right() <= column.right() + px(0.5), "{action} ({width}px): {button:?} outside {column:?}");
                assert!(button.size.height < px(80.), "{action}: {button:?}");
            }
        }
        assert!(seen > 5, "only {seen} lines measured at {width}px");
        cx.simulate_keystrokes("escape");
    }
    // Typing a shortcut: the same button says so; backspace leaves it empty, shown as a dash.
    cx.simulate_resize(size(px(860.), px(1000.)));
    cx.simulate_keystrokes("f1");
    cx.run_until_parked();
    let top = help_sections(&[], &keys::Changes::new())
        .into_iter()
        .flat_map(|(_, rows)| rows)
        .filter_map(|row| row.action)
        .find(|action| cx.debug_bounds(leak(format!("key-{action}"))).is_some_and(|b| b.bottom() < px(900.)))
        .expect("a line with keys");
    let (key, column) = (leak(format!("key-{top}")), leak(format!("key-column-{top}")));
    let before = bounds_of(cx, key);
    let at = before.center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    let waiting = bounds_of(cx, key);
    assert!(waiting.right() <= bounds_of(cx, column).right() + px(0.5), "{waiting:?}");
    assert_ne!(waiting.size, before.size, "the button did not change while waiting");
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    let none = bounds_of(cx, key);
    assert!(none.size.width > px(0.) && none.size.width < before.size.width, "{none:?} {before:?}");
    // Reset all: the original badges are back.
    let at = bounds_of(cx, "keys-reset").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(bounds_of(cx, key).size, before.size);
    fs::remove_dir_all(&root).unwrap();
}

/// The context menu: the badges are pinned right of the label, never over it, in a window as
/// narrow as it gets; an entry with no shortcut draws no badge.
#[gpui::test]
fn keycaps_in_menu(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-keycaps-menu-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("Note.md"), "# Note\n").unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
    cx.run_until_parked();
    for (width, at) in [(1200., 300.), (420., 300.), (420., 0.)] {
        cx.simulate_resize(size(px(width), px(720.)));
        shell.update(cx, |s, cx| {
            s.menu = Some(nav::Menu { at: point(px(at), px(120.)), target: Some(root.join("Note.md")), text: None, commented: false, block: false, ..Default::default() });
            cx.notify();
        });
        cx.run_until_parked();
        // Let the menu's fade-in end.
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.run_until_parked();
        let mut with_keys = 0;
        for label in [tr("Open", "Ouvrir"), tr("Rename", "Renommer"), tr("Move to the trash", "Mettre à la corbeille"), tr("Duplicate", "Dupliquer")] {
            let Some(entry) = cx.debug_bounds(leak(format!("menu-entry-{label}"))) else { continue };
            let caps = bounds_of(cx, leak(format!("menu-keys-{label}")));
            if caps.size.width == px(0.) {
                continue;
            }
            with_keys += 1;
            assert!(caps.right() <= entry.right() + px(0.5) && caps.left() >= entry.left(), "{label}: {caps:?} outside {entry:?}");
            assert!(caps.size.height <= entry.size.height, "{label}: {caps:?} {entry:?}");
            // Nothing from the label's own text runs under the badges: the entry is wide enough for both.
            assert!(entry.size.width >= px(200.) && entry.right() <= px(width) + px(0.5), "{label}: {entry:?} in {width}px");
        }
        assert!(with_keys >= 1, "no entry shows a shortcut at {width}px");
        shell.update(cx, |s, cx| {
            s.menu = None;
            cx.notify();
        });
        cx.run_until_parked();
    }
    fs::remove_dir_all(&root).unwrap();
}

/// The palette and the empty page: a shortcut in the detail of a row, selected or not; the row
/// of an item with no shortcut has no badge.
#[gpui::test]
fn keycaps_in_palette_and_empty_page(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-keycaps-palette-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (_shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
    cx.run_until_parked();
    // The empty page: three shortcuts, each a badge row before its words.
    for i in 0..3 {
        let row = bounds_of(cx, leak(format!("empty-key-{i}")));
        assert!(row.size.height < px(40.), "{row:?}");
    }
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("coffre");
    cx.run_until_parked();
    let mut shown = 0;
    for i in 0..14 {
        let (Some(row), Some(caps)) = (cx.debug_bounds(leak(format!("palette-item-{i}"))), cx.debug_bounds(leak(format!("palette-detail-{i}")))) else { continue };
        shown += 1;
        assert!(caps.left() >= row.left() && caps.right() <= row.right() + px(0.5), "{caps:?} outside {row:?}");
        assert!(caps.size.height <= row.size.height, "{caps:?} {row:?}");
        // The first row is selected, the others not: same badges.
        assert!(caps.size.height > px(10.) || caps.size.width == px(0.));
    }
    assert!(shown >= 1, "no row with a shortcut");
    cx.simulate_keystrokes("escape");
    fs::remove_dir_all(&root).unwrap();
}
