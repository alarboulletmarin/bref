//! The settings page: opened with Ctrl+, as a tab, walked section by section with the keys, a value
//! changed and found in the `settings` file, a search across sections, and the tab kept across a restart.

use crate::platform::config;
use super::*;
use crate::shell::settings_page::{self as page, Row, Section};

fn sel(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext) -> (Section, Option<Row>) {
    shell.read_with(cx, |s, _| (s.settings.section, s.settings.sel))
}

#[gpui::test]
fn the_settings_are_a_tab_walked_by_keyboard(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "settings-page");
    fs::write(root.join("Alpha.md"), "# Alpha\n\nText.\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| s.open(&root.join("Alpha.md"), window, cx));
    cx.run_until_parked();

    // Ctrl+, opens it as a tab of its own, next to the note, with the keys.
    cx.simulate_keystrokes("secondary-,");
    settle(cx);
    let (tabs, on_page) = shell.read_with(cx, |s, _| (s.groups[0].tabs.len(), s.pane().settings()));
    assert_eq!((tabs, on_page), (2, true), "the settings open as a second tab");
    assert!(cx.update(|window, cx| shell.read(cx).settings.focus.is_focused(window)), "the page has the keys");
    assert!(cx.debug_bounds("settings-row-theme").is_some(), "Appearance first, with the theme");
    assert_eq!(shell.read_with(cx, |s, cx| s.tab_name(s.active, cx)), "Réglages");
    // Ctrl+, again brings the same tab back, it does not open a second one.
    shell.update_in(cx, |s, window, cx| {
        let note = s.groups[0].tabs[0];
        s.focus_pane(note, window, cx);
    });
    cx.simulate_keystrokes("secondary-,");
    settle(cx);
    assert_eq!(shell.read_with(cx, |s, _| (s.groups[0].tabs.len(), s.pane().settings())), (2, true));

    // Down walks the rows, into the next section at the end of one; Up comes back.
    cx.simulate_keystrokes("down");
    assert_eq!(sel(&shell, cx), (Section::Appearance, Some(Row::Theme)));
    let appearance = Row::ALL.iter().filter(|r| r.section() == Section::Appearance).count();
    for _ in 0..appearance {
        cx.simulate_keystrokes("down");
    }
    assert_eq!(sel(&shell, cx), (Section::Editor, Some(Row::Page)), "past the last row of Appearance: Editor");
    cx.simulate_keystrokes("up");
    assert_eq!(sel(&shell, cx), (Section::Appearance, Some(Row::Opacity)));

    // Tab goes through every section and comes back to the first; each one shows its rows.
    cx.simulate_keystrokes("tab");
    for section in Section::ALL.iter().skip(1).chain(Section::ALL.iter().take(1)) {
        settle(cx);
        let first = Row::ALL.into_iter().find(|r| r.section() == *section).unwrap();
        assert_eq!(shell.read_with(cx, |s, cx| s.settings_rows(cx).first().copied()), Some(first), "{section:?} shows its rows");
        cx.simulate_keystrokes("tab");
    }
    settle(cx);
    assert_eq!(sel(&shell, cx).0, Section::Editor);
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(sel(&shell, cx), (Section::Appearance, Some(Row::Theme)));

    // A value changed with the keys: applied at once, saved in the file.
    shell.update(cx, |s, cx| s.show_section(Section::Editor, cx));
    cx.simulate_keystrokes("down");
    assert_eq!(sel(&shell, cx).1, Some(Row::Page));
    cx.simulate_keystrokes("right");
    let page = shell.read_with(cx, |s, _| s.theme.page);
    assert_eq!(page, crate::ui::theme::Page::Wide, "normal → wide");
    assert!(config::load(config::File::Settings).contains("page=wide\n"));
    cx.simulate_keystrokes("down down");
    assert_eq!(sel(&shell, cx).1, Some(Row::Bar));
    cx.simulate_keystrokes("space");
    assert!(shell.read_with(cx, |s, _| !s.theme.bar));
    assert!(config::load(config::File::Settings).contains("bar=off\n"));
    cx.simulate_keystrokes("enter");
    assert!(shell.read_with(cx, |s, _| s.theme.bar), "Enter turns it back on");

    // A click on a theme card: the window and the page restyle at once.
    shell.update(cx, |s, cx| s.show_section(Section::Appearance, cx));
    settle(cx);
    let dracula = 1 + THEMES.iter().position(|t| t.0 == "Dracula").unwrap();
    let at = cx.debug_bounds(leak(format!("settings-theme-{dracula}"))).expect("the Dracula card").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    settle(cx);
    assert_eq!(shell.read_with(cx, |s, _| s.prefs.theme.clone()), "Dracula");
    assert!(config::load(config::File::Settings).contains("theme=Dracula\n"));
    let bg = shell.read_with(cx, |s, _| s.theme.bg);
    assert_eq!(bg, gpui::Hsla::from(gpui::rgb(THEMES[dracula - 1].1[0])));

    // A letter typed on the page starts a search across every section; Enter picks the first row found.
    cx.simulate_keystrokes("a n i m");
    settle(cx);
    assert_eq!(shell.read_with(cx, |s, cx| s.settings_rows(cx)), vec![Row::Motion], "only the animations");
    cx.simulate_keystrokes("enter");
    assert_eq!(sel(&shell, cx).1, Some(Row::Motion));
    assert!(cx.update(|window, cx| shell.read(cx).settings.focus.is_focused(window)), "back on the rows");
    cx.simulate_keystrokes("space");
    assert!(config::load(config::File::Settings).contains("motion=off\n"));

    // The file edited by hand: opening the page again shows what it says.
    let edited = config::load(config::File::Settings).replace("theme=Dracula", "theme=Nord");
    config::save(config::File::Settings, &edited).unwrap();
    cx.simulate_keystrokes("secondary-,");
    settle(cx);
    assert_eq!(shell.read_with(cx, |s, _| s.prefs.theme.clone()), "Nord");

    // The tab is kept with the layout, and restored as the settings.
    let state = shell.read_with(cx, |s, _| s.panes_state());
    assert!(state.contains(page::TAB), "{state}");

    // Escape clears the search, then closes the tab: the note is back.
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(shell.read_with(cx, |s, _| s.pane().settings()), "the first Escape clears the search");
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(shell.read_with(cx, |s, _| (s.groups[0].tabs.len(), s.pane().settings())), (1, false));
    assert_eq!(shell.read_with(cx, |s, _| s.pane().path.clone()), Some(root.join("Alpha.md")));
}

#[gpui::test]
fn a_saved_settings_tab_comes_back(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "settings-restore");
    fs::write(root.join("Alpha.md"), "# Alpha\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| s.open(&root.join("Alpha.md"), window, cx));
    cx.simulate_keystrokes("secondary-,");
    settle(cx);
    let state = shell.read_with(cx, |s, _| s.panes_state());
    config::save(config::File::Panes, &state).unwrap();
    let (again, cx2) = cx.add_window_view({
        let root = root.to_path_buf();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx2.run_until_parked();
    let tabs = again.read_with(cx2, |s, _| s.groups[0].tabs.iter().map(|&t| s.panes[t].settings()).collect::<Vec<_>>());
    assert_eq!(tabs, vec![false, true], "the note, then the settings");
    assert!(again.read_with(cx2, |s, _| s.settings.find.is_some()), "restored with its search field");
}

#[gpui::test]
fn the_rail_gear_and_the_palette_open_it(cx: &mut TestAppContext) {
    let (_root, shell, cx, _config) = start(cx, "settings-gear");
    settle(cx);
    let at = cx.debug_bounds("nav-settings").expect("the gear on the rail").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    settle(cx);
    assert!(shell.read_with(cx, |s, _| s.pane().settings()));
    // Typed on the page: a search; Tab leaves the field for the first setting found.
    cx.simulate_keystrokes("d r a c");
    assert!(cx.update(|window, cx| !shell.read(cx).settings.focus.is_focused(window)), "the field has the keys");
    cx.simulate_keystrokes("tab");
    assert_eq!(sel(&shell, cx).1, Some(Row::Theme));
    assert!(cx.update(|window, cx| shell.read(cx).settings.focus.is_focused(window)), "back on the rows");
    // The agents' section: the configuration to copy names this program, read-only when asked.
    shell.update(cx, |s, cx| s.show_section(Section::Agents, cx));
    settle(cx);
    let at = cx.debug_bounds("settings-access-read-only").expect("read only").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    let at = cx.debug_bounds("settings-client-copy").expect("copy").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    let copied = cx.read_from_clipboard().and_then(|item| item.text()).unwrap_or_default();
    assert!(copied.contains("\"--read-only\"") && copied.contains("mcpServers"), "{copied}");
}

#[gpui::test]
fn narrow_and_large_nothing_is_cut(cx: &mut TestAppContext) {
    let (_root, shell, cx, _config) = start(cx, "settings-narrow");
    shell.update_in(cx, |s, window, cx| s.open_settings(None, window, cx));
    for (width, size) in [(360., 16.), (1200., 24.), (900., 13.6)] {
        cx.simulate_resize(gpui::size(px(width), px(700.)));
        shell.update_in(cx, |s, window, cx| s.resize_text(Some(size - s.prefs.size), window, cx));
        settle(cx);
        let page = cx.debug_bounds("settings").expect("the page");
        for section in Section::ALL {
            shell.update(cx, |s, cx| s.show_section(section, cx));
            settle(cx);
            for row in Row::ALL.into_iter().filter(|r| r.section() == section) {
                let Some(at) = cx.debug_bounds(leak(format!("settings-row-{}", row.key()))) else { continue };
                assert!(at.left() >= page.left() && at.right() <= page.right() + px(0.5), "{width} px, {size} px: {row:?} {at:?} out of {page:?}");
            }
        }
        // Narrow: the sections go above the rows instead of beside them.
        let (nav, content) = (cx.debug_bounds("settings-sections").unwrap(), cx.debug_bounds("settings-content").unwrap());
        assert_eq!(nav.bottom() <= content.top(), width < 500., "{width} px: sections above when narrow");
    }
}

/// A row's line never runs under its control: the control is beside the text or under it, at every
/// width.
#[gpui::test]
fn a_line_never_runs_under_its_control(cx: &mut TestAppContext) {
    let (_root, shell, cx, _config) = start(cx, "settings-overlap");
    shell.update_in(cx, |s, window, cx| s.open_settings(None, window, cx));
    for width in [360., 600., 790., 900., 1200., 1600.] {
        cx.simulate_resize(gpui::size(px(width), px(700.)));
        settle(cx);
        for section in Section::ALL {
            shell.update(cx, |s, cx| s.show_section(section, cx));
            settle(cx);
            for row in Row::ALL.into_iter().filter(|r| r.section() == section) {
                let key = row.key();
                let (Some(line), Some(control)) = (cx.debug_bounds(leak(format!("settings-line-{key}"))), cx.debug_bounds(leak(format!("settings-control-{key}")))) else { continue };
                let row_box = cx.debug_bounds(leak(format!("settings-row-{key}"))).unwrap();
                let beside = line.right() <= control.left() + px(0.5);
                let under = line.bottom() <= control.top() + px(0.5);
                assert!(beside || under, "{width} px: {row:?} line {line:?} runs under its control {control:?}");
                assert!(line.right() <= row_box.right() + px(0.5), "{width} px: {row:?} line {line:?} leaves its row {row_box:?}");
            }
        }
    }
}

/// The accent: a preset clicked, a hex typed (short form written back long), a word refused, Escape back to
/// the kept one, a pale colour said to be shown darker, back to the theme's from the keys, and from the palette.
#[gpui::test]
fn an_accent_over_any_theme(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "settings-accent");
    fs::write(root.join("Alpha.md"), "# Alpha\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| s.open(&root.join("Alpha.md"), window, cx));
    shell.update_in(cx, |s, window, cx| {
        s.prefs.theme = "Bref Light".into();
        s.restyle(window, cx);
        s.open_settings(Some(Section::Appearance), window, cx);
    });
    settle(cx);
    let accent = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.prefs.accent.clone());
    let typed = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.settings.accent.as_ref().unwrap().read(cx).text().to_string());
    let own = shell.read_with(cx, |s, _| s.theme.accent);
    assert!(cx.debug_bounds("settings-accent-said").is_none(), "nothing to say with the theme's accent");

    // A preset clicked: saved, applied at once, shown in the field.
    let at = cx.debug_bounds("settings-accent-#0090ff").expect("the blue swatch").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    settle(cx);
    assert_eq!(accent(cx), "#0090ff");
    assert!(config::load(config::File::Settings).contains("accent=#0090ff\n"));
    let blue = shell.read_with(cx, |s, _| s.theme.accent);
    assert!(blue != own && (blue.h - gpui::Hsla::from(gpui::rgb(0x0090ff)).h).abs() < 0.01, "links and tags in blue");
    assert_eq!(typed(cx), "#0090ff");

    // A short hex typed in the field: kept long, the keys back on the page.
    let field = cx.debug_bounds("settings-accent-field").expect("the hex field").center();
    cx.simulate_click(field, gpui::Modifiers::none());
    shell.update(cx, |s, cx| s.settings.accent.as_ref().unwrap().update(cx, |f, cx| f.select_all(cx)));
    cx.simulate_input("#f80");
    assert!(cx.debug_bounds("settings-accent-typed").is_some(), "a swatch of what is typed");
    cx.simulate_keystrokes("enter");
    settle(cx);
    assert_eq!((accent(cx), typed(cx)), ("#ff8800".to_string(), "#ff8800".to_string()));
    assert!(config::load(config::File::Settings).contains("accent=#ff8800\n"));
    assert!(cx.update(|window, cx| shell.read(cx).settings.focus.is_focused(window)), "Enter gives the keys back to the page");

    // A word is no colour: refused and said, nothing saved; Escape puts the kept accent back.
    cx.simulate_click(field, gpui::Modifiers::none());
    shell.update(cx, |s, cx| s.settings.accent.as_ref().unwrap().update(cx, |f, cx| f.select_all(cx)));
    cx.simulate_input("rouge");
    cx.simulate_keystrokes("enter");
    settle(cx);
    assert_eq!(accent(cx), "#ff8800", "nothing saved");
    assert!(shell.read_with(cx, |s, _| s.settings.accent_bad) && cx.debug_bounds("settings-accent-said").is_some(), "said in the row");
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(typed(cx), "#ff8800");
    assert!(!shell.read_with(cx, |s, _| s.settings.accent_bad));

    // A pale yellow cannot be read on Bref Light as it is: shown darker, and the row says which.
    cx.simulate_click(field, gpui::Modifiers::none());
    shell.update(cx, |s, cx| s.settings.accent.as_ref().unwrap().update(cx, |f, cx| f.select_all(cx)));
    cx.simulate_input("#ffff99");
    cx.simulate_keystrokes("enter");
    settle(cx);
    let shown = shell.read_with(cx, |s, _| crate::ui::theme::hex(s.theme.accent));
    assert_ne!(shown, "#ffff99");
    assert!(cx.debug_bounds("settings-accent-said").is_some(), "the colour shown is said");

    // From the keys: Enter goes to the next choice, the theme's own first when the accent is none of the presets.
    shell.update(cx, |s, _| s.settings.sel = Some(Row::Accent));
    cx.simulate_keystrokes("enter");
    settle(cx);
    assert_eq!(accent(cx), "", "back to the theme's");
    assert!(!config::load(config::File::Settings).contains("accent="));
    assert_eq!(shell.read_with(cx, |s, _| s.theme.accent), own);
    cx.simulate_keystrokes("right");
    assert_eq!(accent(cx), "#e5484d", "Right: the first preset");

    // From the palette: browsing previews, Escape takes it back, Enter keeps it.
    let choose = |cx: &mut gpui::VisualTestContext, keys: &str| {
        shell.update_in(cx, |s, window, cx| s.choose_setting(Setting::Accent, window, cx));
        cx.run_until_parked();
        cx.simulate_keystrokes(keys);
        cx.run_until_parked();
    };
    choose(cx, "up");
    assert_eq!(accent(cx), "", "the theme's own, previewed");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(accent(cx), "#e5484d", "Escape: the one before");
    choose(cx, "up enter");
    assert_eq!(accent(cx), "");
    assert_eq!(shell.read_with(cx, |s, _| s.theme.accent), own);
}
