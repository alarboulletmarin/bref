//! The note's find bar: one bar, two modes, its fields (cursor, selection, input method), its measures.

use super::*;
use gpui::{Bounds, Entity, EntityInputHandler, Pixels};

type Cx = gpui::VisualTestContext;

fn finding(shell: &Entity<Shell>, cx: &mut Cx) -> Option<(String, usize, usize)> {
    shell.read_with(cx, |s, cx| s.pane().editor.read(cx).finding(cx))
}

/// The replace line shows.
fn replacing(shell: &Entity<Shell>, cx: &mut Cx) -> Option<bool> {
    shell.update_in(cx, |s, window, cx| s.pane().editor.read(cx).find_mode(window, cx).map(|(replace, _)| replace))
}

/// What has the keys: the search, the replacement, the note.
fn keys_in(shell: &Entity<Shell>, cx: &mut Cx) -> &'static str {
    shell.update_in(cx, |s, window, cx| {
        let editor = s.pane().editor.read(cx);
        match editor.find_fields() {
            Some((query, _)) if query.focus_handle(cx).is_focused(window) => "query",
            Some((_, with)) if with.focus_handle(cx).is_focused(window) => "with",
            _ if s.pane().editor.focus_handle(cx).is_focused(window) => "note",
            _ => "elsewhere",
        }
    })
}

fn texts(shell: &Entity<Shell>, cx: &mut Cx) -> (String, String) {
    shell.read_with(cx, |s, cx| {
        let (query, with) = s.pane().editor.read(cx).find_fields().unwrap();
        (query.read(cx).line.text.clone(), with.read(cx).line.text.clone())
    })
}

fn note_text(shell: &Entity<Shell>, cx: &mut Cx) -> String {
    shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string())
}

fn load(shell: &Entity<Shell>, cx: &mut Cx, text: &str) {
    shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.load(text.to_string(), 0, cx)));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn apart(a: Bounds<Pixels>, b: Bounds<Pixels>) -> bool {
    a.right() <= b.left() || b.right() <= a.left() || a.bottom() <= b.top() || b.bottom() <= a.top()
}

fn inside(a: Bounds<Pixels>, b: Bounds<Pixels>) -> bool {
    a.left() >= b.left() && a.right() <= b.right() && a.top() >= b.top() && a.bottom() <= b.bottom()
}

fn opened<'a>(cx: &'a mut TestAppContext, name: &str, text: &str) -> (PathBuf, Entity<Shell>, &'a mut Cx, std::sync::MutexGuard<'static, ()>) {
    let (root, shell, cx, config) = start(cx, name);
    fs::write(root.join("Note.md"), text).unwrap();
    shell.update(cx, |s, cx| {
        s.rescan(&root, cx);
        s.open_note(&root.join("Note.md"), cx);
    });
    cx.run_until_parked();
    (root, shell, cx, config)
}

/// Ctrl+F and Ctrl+R switch the bar's mode without closing it: the search, its settings and the current match
/// stay, and the keys go to the field of the mode. The chevron does the same with the mouse. Escape in the
/// middle of typing closes it, the search kept for next time; Replace all is one undo step, redone whole.
#[gpui::test]
fn one_bar_two_modes(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = opened(cx, "find-modes", "Été été ÉTÉ été\n");
    cx.simulate_keystrokes("secondary-f");
    assert_eq!(keys_in(&shell, cx), "query");
    cx.simulate_input("été");
    cx.simulate_keystrokes("alt-c enter");
    let kept = finding(&shell, cx);
    assert_eq!(kept, Some(("été".into(), 2, 2)));
    for (key, replace, field) in [("secondary-r", true, "with"), ("secondary-f", false, "query"), ("secondary-r", true, "with"), ("secondary-f", false, "query")] {
        cx.simulate_keystrokes(key);
        assert_eq!((finding(&shell, cx), replacing(&shell, cx), keys_in(&shell, cx)), (kept.clone(), Some(replace), field), "{key}");
    }
    // Twice quickly: still open, on the same search.
    cx.simulate_keystrokes("secondary-f secondary-f");
    assert_eq!(finding(&shell, cx), kept);
    // Typed text lands in the field that has the keys.
    cx.simulate_keystrokes("secondary-r");
    cx.simulate_input("hiver");
    assert_eq!(texts(&shell, cx), ("été".into(), "hiver".into()));
    // The chevron: the replace line hides, the keys go to the search; shown again, to the replacement.
    for (replace, field) in [(false, "query"), (true, "with")] {
        let at = cx.debug_bounds("find-chevron").unwrap().center();
        cx.simulate_click(at, gpui::Modifiers::none());
        assert_eq!((replacing(&shell, cx), keys_in(&shell, cx), finding(&shell, cx)), (Some(replace), field, kept.clone()));
    }
    // Replace all: one undo step, redone whole.
    cx.simulate_keystrokes("secondary-enter");
    assert_eq!(note_text(&shell, cx), "Été hiver ÉTÉ hiver\n");
    cx.simulate_keystrokes("escape");
    assert_eq!((finding(&shell, cx), keys_in(&shell, cx)), (None, "note"));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(note_text(&shell, cx), "Été été ÉTÉ été\n");
    cx.simulate_keystrokes("secondary-shift-z");
    assert_eq!(note_text(&shell, cx), "Été hiver ÉTÉ hiver\n");
    cx.simulate_keystrokes("secondary-z");
    // Escape in the middle of typing: closed, the note has the keys; reopened, the search is back, all
    // selected, and the first keystroke replaces it.
    cx.simulate_keystrokes("secondary-f");
    cx.simulate_input("ÉT");
    cx.simulate_keystrokes("escape");
    assert_eq!((finding(&shell, cx), keys_in(&shell, cx)), (None, "note"));
    cx.simulate_keystrokes("secondary-f");
    assert_eq!(finding(&shell, cx).map(|f| f.0), Some("ÉT".into()));
    cx.simulate_input("ÉTÉ");
    assert_eq!(finding(&shell, cx), Some(("ÉTÉ".into(), 1, 1)));
    let _ = fs::remove_dir_all(&root);
}

/// The search field takes an input method: composed text shows as typed and searches once committed; a dead
/// key followed by a letter gives one accented letter.
#[gpui::test]
fn the_search_takes_an_input_method(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = opened(cx, "find-ime", "café 東京 tôt\n");
    cx.simulate_keystrokes("secondary-f");
    let query = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).find_fields().unwrap().0);
    // Japanese: « とうきょう » composed, then committed as « 東京 ».
    cx.update(|window, cx| query.update(cx, |field, cx| field.replace_and_mark_text_in_range(None, "とうきょう", Some(5..5), window, cx)));
    assert_eq!(cx.update(|window, cx| query.update(cx, |field, cx| field.marked_text_range(window, cx))), Some(0..5));
    cx.update(|window, cx| query.update(cx, |field, cx| field.replace_text_in_range(None, "東京", window, cx)));
    assert_eq!(finding(&shell, cx), Some(("東京".into(), 1, 1)));
    // A dead key: « ^ » marked, then « ô » in its place.
    cx.simulate_keystrokes("secondary-a backspace");
    cx.update(|window, cx| query.update(cx, |field, cx| field.replace_and_mark_text_in_range(None, "^", None, window, cx)));
    cx.update(|window, cx| query.update(cx, |field, cx| field.replace_text_in_range(None, "ô", window, cx)));
    cx.simulate_input("t");
    assert_eq!(finding(&shell, cx), Some(("ôt".into(), 1, 1)));
    // The cursor moves in the field: Home, then a letter at the start.
    cx.simulate_keystrokes("home");
    cx.simulate_input("t");
    assert_eq!(finding(&shell, cx), Some(("tôt".into(), 1, 1)));
    let _ = fs::remove_dir_all(&root);
}

/// The bar stays in its pane at every width, the count never overlaps the toggles (none to 10 000 matches,
/// and the longest message), and a long search scrolls in its field instead of widening the bar.
#[gpui::test]
fn the_bar_holds_its_place(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = opened(cx, "find-measures", "a\n");
    cx.simulate_keystrokes("secondary-r");
    cx.simulate_input("a");
    let check = |cx: &mut Cx, what: &str| {
        let (bar, pane) = (cx.debug_bounds("find-bar").unwrap(), cx.debug_bounds("pane-0").unwrap());
        assert!(inside(bar, pane), "{what}: the bar {bar:?} in the pane {pane:?}");
        let parts = ["find-chevron", "find-query", "find-count", "find-case", "find-word", "find-regex", "find-with", "replace-one", "replace-all", "replace-vault"]
            .map(|id| (id, cx.debug_bounds(id).unwrap()));
        for (i, (id, part)) in parts.iter().enumerate() {
            assert!(inside(*part, bar), "{what}: {id} {part:?} in the bar {bar:?}");
            for (other, them) in &parts[i + 1..] {
                assert!(apart(*part, *them), "{what}: {id} {part:?} over {other} {them:?}");
            }
        }
    };
    let geometries = [(nav::Panel::Rail, 260., "secondary-0"), (nav::Panel::Split, 260., "secondary-0"), (nav::Panel::Split, 600., "secondary-0"), (nav::Panel::Rail, 260., &"secondary-= ".repeat(16))];
    for (i, (panel, list_w, zoom)) in geometries.into_iter().enumerate() {
        shell.update(cx, |s, cx| {
            s.nav.panel = panel;
            s.nav.list_w = px(list_w);
            cx.notify();
        });
        cx.simulate_keystrokes(zoom.trim_end());
        let zoomed = shell.read_with(cx, |s, _| s.theme.size);
        for width in [420., 860., 1600.] {
            cx.simulate_resize(size(px(width), px(720.)));
            // Every count in the first geometry; in the others « Invalid expression » is the widest text anyway.
            let counts: &[usize] = if i == 0 { &[0, 1, 9, 10, 99, 100, 1_000, 10_000] } else { &[0, 1_000] };
            for &n in counts {
                load(&shell, cx, &if n == 0 { "b".to_string() } else { "a ".repeat(n) });
                assert_eq!(finding(&shell, cx).map(|f| f.2), Some(n));
                if n > 0 {
                    cx.simulate_keystrokes("shift-enter");
                    assert_eq!(finding(&shell, cx), Some(("a".into(), n, n)), "the last match");
                }
                check(cx, &format!("{panel:?} {list_w} zoom {zoomed}, {width} px, {n}"));
            }
            // The longest message: a regular expression that does not compile.
            cx.simulate_keystrokes("secondary-f");
            cx.simulate_input("(");
            cx.simulate_keystrokes("alt-r secondary-r");
            check(cx, &format!("{panel:?} {list_w} zoom {zoomed}, {width} px, invalid"));
            cx.simulate_keystrokes("alt-r secondary-f backspace secondary-r");
        }
    }
    cx.simulate_keystrokes("secondary-0");
    shell.update(cx, |s, cx| {
        s.nav.panel = nav::Panel::Rail;
        cx.notify();
    });
    // A search of 300 characters with no space: the bar keeps its width, the field scrolls.
    cx.simulate_resize(size(px(860.), px(720.)));
    load(&shell, cx, "a\n");
    cx.simulate_keystrokes("secondary-f");
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let short = cx.debug_bounds("find-bar").unwrap().size;
    cx.simulate_input(&"x".repeat(300));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("find-bar").unwrap().size, short);
    // The field follows the cursor to the end, then back to the start.
    let query = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).find_fields().unwrap().0);
    let scrolled = |cx: &mut Cx| query.read_with(cx, |field, _| field.scrolled());
    let field = cx.debug_bounds("find-query").unwrap();
    assert!(scrolled(cx) > field.size.width, "{:?}", scrolled(cx));
    cx.simulate_keystrokes("home");
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert_eq!(scrolled(cx), px(0.));
    let _ = fs::remove_dir_all(&root);
}

/// Replacing at the edges: an empty replacement, one that holds the search, `$1` with no group, the last
/// match (it goes round to the first), regular expressions that match nothing wide (`^`, `a*`).
#[gpui::test]
fn replace_at_the_edges(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = opened(cx, "find-edges", "a b a\n");
    // `a` → `aa`: each one once, no endless loop.
    cx.simulate_keystrokes("secondary-r");
    cx.simulate_input("a");
    cx.simulate_keystrokes("secondary-r");
    cx.simulate_input("aa");
    cx.simulate_keystrokes("secondary-enter");
    assert_eq!(note_text(&shell, cx), "aa b aa\n");
    // Empty replacement: removed.
    cx.simulate_keystrokes("secondary-a backspace secondary-enter");
    assert_eq!(note_text(&shell, cx), " b \n");
    // The last match replaced, the next is the first.
    load(&shell, cx, "a b a\n");
    cx.simulate_keystrokes("secondary-f shift-enter");
    assert_eq!(finding(&shell, cx), Some(("a".into(), 2, 2)));
    cx.simulate_keystrokes("secondary-r");
    cx.simulate_input("c");
    cx.simulate_keystrokes("enter");
    assert_eq!((note_text(&shell, cx), finding(&shell, cx)), ("a b c\n".into(), Some(("a".into(), 1, 1))));
    // `$1` with no group: nothing in its place.
    cx.simulate_keystrokes("secondary-f secondary-a backspace");
    cx.simulate_input("b");
    cx.simulate_keystrokes("alt-r secondary-r secondary-a backspace");
    cx.simulate_input("[$1]");
    cx.simulate_keystrokes("secondary-enter");
    assert_eq!(note_text(&shell, cx), "a [] c\n");
    // Regular expressions that can match empty: an empty match is no match, nothing hangs.
    for (query, n) in [("^", 0), ("a*", 1)] {
        cx.simulate_keystrokes("secondary-f secondary-a backspace");
        cx.simulate_input(query);
        assert_eq!(finding(&shell, cx).map(|f| f.2), Some(n), "{query}");
    }
    let _ = fs::remove_dir_all(&root);
}

/// Ctrl+R replaces in the note and nowhere else; the recent notes moved to Ctrl+Shift+T, from the note as from
/// the palette; someone who gave Ctrl+R to something else keeps it.
#[gpui::test]
fn the_keys_of_the_bar(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = opened(cx, "find-keys", "a\n");
    fs::write(root.join("Table.csv"), "a,b\n1,2\n").unwrap();
    let mode = |cx: &mut Cx| shell.read_with(cx, |s, _| s.nav.mode);
    cx.simulate_keystrokes("secondary-shift-t");
    assert_eq!(mode(cx), nav::Mode::Recent);
    cx.simulate_keystrokes("secondary-e");
    // In the palette, Ctrl+R does nothing; Ctrl+Shift+T still shows the recent notes.
    cx.simulate_keystrokes("secondary-p secondary-r");
    assert!(shell.read_with(cx, |s, cx| s.palette.is_some() && s.pane().editor.read(cx).finding(cx).is_none()));
    assert_ne!(mode(cx), nav::Mode::Recent);
    cx.simulate_keystrokes("secondary-shift-t");
    assert_eq!(mode(cx), nav::Mode::Recent);
    cx.simulate_keystrokes("escape secondary-e");
    // In a table view: nothing either.
    shell.update(cx, |s, cx| s.open_note(&root.join("Table.csv"), cx));
    cx.run_until_parked();
    cx.simulate_keystrokes("secondary-r");
    assert_ne!(mode(cx), nav::Mode::Recent);
    // Ctrl+R given to the recent notes in the keys file: it is theirs, in the note too.
    shell.update(cx, |s, cx| {
        s.open_note(&root.join("Note.md"), cx);
        s.key_set("nav::ShowRecent", "secondary-r".into(), &[], cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("secondary-r");
    assert_eq!(mode(cx), nav::Mode::Recent);
    assert_eq!(finding(&shell, cx), None);
    let _ = fs::remove_dir_all(&root);
}

/// Two panes: the bar belongs to the pane where it was opened.
#[gpui::test]
fn the_bar_belongs_to_its_pane(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = opened(cx, "find-panes", "a\n");
    fs::write(root.join("Other.md"), "b\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.simulate_keystrokes("secondary-\\");
    cx.simulate_input("other");
    cx.simulate_keystrokes("enter");
    assert_eq!(shell.read_with(cx, |s, _| (s.active, s.panes.len())), (1, 2));
    cx.simulate_keystrokes("secondary-f");
    cx.simulate_input("b");
    let found = |cx: &mut Cx, pane: usize| shell.read_with(cx, |s, cx| s.panes[pane].editor.read(cx).finding(cx));
    assert_eq!((found(cx, 0), found(cx, 1)), (None, Some(("b".into(), 1, 1))));
    let (bar, left, right) = (cx.debug_bounds("find-bar").unwrap(), cx.debug_bounds("pane-0").unwrap(), cx.debug_bounds("pane-1").unwrap());
    assert!(inside(bar, right) && apart(bar, left), "{bar:?} in {right:?}");
    let _ = fs::remove_dir_all(&root);
}
