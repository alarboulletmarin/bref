//! Quick copy, the help panel, editable shortcuts, themes, fonts and text size.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Quick copy of the whole note.
    cx.simulate_keystrokes("secondary-shift-c");
    let clipboard = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(clipboard.as_deref(), Some("# Idées\n\n"));


    // Help panel: F1 opens it, Escape closes it and gives control back to the editor.
    cx.simulate_keystrokes("f1");
    assert!(shell.read_with(cx, |s, _| s.help.open));
    // Typing goes to the search field, which filters the lines: keys or effect, ignoring
    // accents.
    let rows = |cx: &mut gpui::VisualTestContext| {
        shell.read_with(cx, |s, cx| s.help_rows(cx).into_iter().flat_map(|(_, rows)| rows).collect::<Vec<_>>())
    };
    let all = rows(cx).len();
    cx.simulate_input("cOmmente");
    let found = rows(cx);
    assert!(!found.is_empty() && found.len() < all, "{found:?}");
    assert!(found.iter().all(|row| crate::lang::fold(row.effect).contains("commente")), "{found:?}");
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_input("maj+f");
    assert!(rows(cx).iter().any(|row| row.keys.ends_with("Maj+F")));
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_input("zzz");
    assert!(rows(cx).is_empty());
    assert_eq!(text(cx), "# Idées\n\n", "nothing is typed in the note");
    // Escape first clears the field, then closes the panel.
    cx.simulate_keystrokes("escape");
    assert!(shell.read_with(cx, |s, _| s.help.open));
    assert_eq!(rows(cx).len(), all);
    cx.simulate_keystrokes("escape");
    assert!(!shell.read_with(cx, |s, _| s.help.open));
    cx.simulate_input("ok");
    assert_eq!(text(cx), "# Idées\n\nok");

    // Editable shortcuts: Enter on a help line waits for keys, the
    // following ones become the shortcut, right away and in the `keys` file.
    let (mode, panel) = shell.read_with(cx, |s, _| (s.nav.mode, s.nav.panel));
    let keys_of = |cx: &mut gpui::VisualTestContext, action: &str| {
        let row = rows(cx).into_iter().find(|row| row.action == Some(action)).unwrap();
        (row.keys, row.changed)
    };
    let editing = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.help.edit.as_ref().map(|edit| (edit.action, edit.issue.clone())));
    let graph = format!("{MOD}+G");
    cx.simulate_keystrokes("f1");
    cx.simulate_input("graphe des notes");
    assert_eq!(keys_of(cx, "nav::ShowGraph"), (graph.clone(), false));
    cx.simulate_keystrokes("down enter");
    assert_eq!(editing(cx), Some(("nav::ShowGraph", None)));
    // Refused, with the reason: a single key that types, a key reserved for text.
    cx.simulate_keystrokes("a");
    assert!(matches!(editing(cx), Some((_, Some(KeyIssue::Refused(_))))));
    assert_eq!(shell.read_with(cx, |s, cx| s.help.find.as_ref().unwrap().read(cx).line.text.clone()), "graphe des notes", "the key is not typed");
    cx.simulate_keystrokes("enter");
    assert!(matches!(editing(cx), Some((_, Some(KeyIssue::Refused(_))))));
    cx.simulate_keystrokes("secondary-shift-y");
    assert_eq!(editing(cx), None);
    assert_eq!(keys_of(cx, "nav::ShowGraph"), (format!("{MOD}+Maj+Y"), true));
    assert_eq!(vault::load_file("keys"), "nav::ShowGraph=secondary-shift-y\n");
    cx.simulate_keystrokes("escape escape");
    assert!(!shell.read_with(cx, |s, _| s.help.open));
    // Without restarting: the old keys do nothing anymore, the new ones act.
    cx.simulate_keystrokes("secondary-e");
    assert_eq!(shell.read_with(cx, |s, _| s.nav.mode), nav::Mode::Tree);
    cx.simulate_keystrokes("secondary-g");
    assert_eq!(shell.read_with(cx, |s, _| s.nav.mode), nav::Mode::Tree);
    cx.simulate_keystrokes("secondary-shift-y");
    assert_eq!(shell.read_with(cx, |s, _| s.nav.mode), nav::Mode::Graph);
    // Keys already taken: the line says by what, and nothing is taken without « remplacer ».
    cx.simulate_keystrokes("f1");
    cx.simulate_input("graphe des notes");
    cx.simulate_keystrokes("down enter secondary-p");
    assert_eq!(editing(cx), Some(("nav::ShowGraph", Some(KeyIssue::Taken(vec!["app::OpenPalette"])))));
    assert!(shell.read_with(cx, |s, _| s.palette.is_none()));
    assert_eq!(keys_of(cx, "nav::ShowGraph").0, format!("{MOD}+Maj+Y"));
    // Enter replaces: the other action loses its shortcut, and the list shows it.
    cx.simulate_keystrokes("enter");
    assert_eq!(editing(cx), None);
    assert_eq!(keys_of(cx, "nav::ShowGraph"), (format!("{MOD}+P"), true));
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_input("filtrer par #tag");
    assert_eq!(keys_of(cx, "app::OpenPalette"), (String::new(), true));
    cx.simulate_keystrokes("escape escape secondary-e secondary-p");
    assert_eq!(shell.read_with(cx, |s, _| (s.nav.mode, s.palette.is_none())), (nav::Mode::Graph, true));
    // Backspace: no shortcut. Escape during capture cancels only it.
    cx.simulate_keystrokes("f1");
    cx.simulate_input("graphe des notes");
    cx.simulate_keystrokes("down enter backspace");
    assert_eq!(keys_of(cx, "nav::ShowGraph"), (String::new(), true));
    cx.simulate_keystrokes("enter escape");
    assert_eq!((editing(cx), shell.read_with(cx, |s, _| s.help.open)), (None, true));
    assert_eq!(rows(cx).len(), 1, "the search did not move");
    // A click on the keys waits too; « défaut » restores the original ones.
    let at = cx.debug_bounds("key-nav::ShowGraph").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(editing(cx), Some(("nav::ShowGraph", None)));
    cx.simulate_keystrokes("escape");
    let at = cx.debug_bounds("key-reset-nav::ShowGraph").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(keys_of(cx, "nav::ShowGraph"), (graph, false));
    assert_eq!(vault::load_file("keys"), "app::OpenPalette=\n");
    // « Tout rétablir »: nothing changed anymore.
    let at = cx.debug_bounds("keys-reset").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert!(shell.read_with(cx, |s, _| s.changes.is_empty()));
    assert_eq!(vault::load_file("keys"), "");
    cx.simulate_keystrokes("escape escape secondary-p");
    assert!(shell.read_with(cx, |s, _| s.palette.is_some()));
    cx.simulate_keystrokes("escape");
    shell.update(cx, |s, cx| {
        s.nav.show_alone(mode);
        s.nav.panel = panel;
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(text(cx), "# Idées\n\nok");

    // Theme: the theme list (rail icon, palette), the one being browsed applies as a
    // preview; Escape goes back to the previous one, Enter keeps the choice and remembers it.
    let look = |cx: &mut gpui::VisualTestContext| {
        shell.read_with(cx, |s, _| (s.prefs.theme.clone(), s.theme.bg))
    };
    let system = look(cx);
    cx.dispatch_action(ChooseTheme);
    cx.simulate_input("drac");
    assert_eq!(look(cx), ("Dracula".to_string(), Hsla::from(gpui::rgb(0x282a36))));
    cx.simulate_keystrokes("escape");
    assert_eq!(look(cx), system);
    cx.dispatch_action(ChooseTheme);
    cx.simulate_keystrokes("down down enter");
    assert_eq!(look(cx), ("Bref Light".to_string(), Hsla::from(gpui::rgb(0xfbfaf8))));
    // Code font, from the palette; text size from the keyboard.
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("police du");
    cx.simulate_keystrokes("down enter");
    cx.simulate_input("systemui");
    cx.simulate_keystrokes("enter");
    assert_eq!(mono(), ".SystemUIFont");
    cx.simulate_keystrokes("secondary-= secondary-= secondary--");
    assert_eq!(shell.read_with(cx, |s, _| s.theme.size), 17.);
    assert_eq!(vault::load_settings(), "theme=Bref Light\nfont=\nmono=.SystemUIFont\nsize=17\n");
    cx.simulate_keystrokes("secondary-0");
    assert_eq!(shell.read_with(cx, |s, _| s.theme.size), 16.);
    cx.simulate_input("!");
    assert_eq!(text(cx), "# Idées\n\nok!");
}
