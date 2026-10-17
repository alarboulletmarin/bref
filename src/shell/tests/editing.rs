//! Editing journeys: `/` commands, block handles, inline formatting and its bar, templates.

use super::*;

/// What can be done is found without knowing it: the command palette (F10, or ">"), which
/// searches them by what they do, the view's first; help opened on the view; a table's bar.
#[gpui::test]
fn commands_are_found(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-commands-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("data.csv"), "nom;age\nAna;31\nBob;27\n").unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx.run_until_parked();
    let text = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string());
    shell.update_in(cx, |s, window, cx| window.focus(&s.pane().editor.focus_handle(cx)));
    cx.simulate_input("mot");
    let runs = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.palette.as_ref().map(|p| p.read(cx).runs()).unwrap_or_default());
    let mode = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.palette.as_ref().map(|p| p.read(cx).mode()));

    // F10 opens the command palette: only the commands the note can do (not those
    // of a grid), under the name of what is being viewed.
    cx.simulate_keystrokes("f10");
    assert_eq!(mode(cx), Some((true, "Note")));
    let all = runs(cx);
    assert!(all.contains(&"app::NewNote") && all.contains(&"editor::Bold"), "{all:?}");
    assert!(!all.contains(&"sheet::Copy") && !all.contains(&"canvas::Erase"));
    // Escape closes it and gives input back to the note.
    cx.simulate_keystrokes("escape");
    assert_eq!(mode(cx), None);
    assert!(shell.update_in(cx, |s, window, cx| s.pane().editor.focus_handle(cx).is_focused(window)));
    // Typing filters by what the command does; Enter runs it, on the note.
    cx.simulate_keystrokes("secondary-a f10");
    cx.simulate_input("gras");
    assert_eq!(runs(cx), ["editor::Bold"]);
    cx.simulate_keystrokes("enter");
    assert_eq!(mode(cx), None);
    assert_eq!(text(cx), "**mot**");
    // The rail's first button opens it too; a click on a command runs it.
    let at = cx.debug_bounds("nav-commands").expect("le bouton du rail").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(mode(cx).map(|m| m.0), Some(true));
    cx.simulate_input("raccourcis");
    let at = cx.debug_bounds("command-app::ToggleHelp").expect("la commande est affichée").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert!(shell.read_with(cx, |s, _| s.palette.is_none() && s.help.open));
    shell.update_in(cx, |s, window, cx| s.set_help(false, window, cx));

    // In the notes palette, a leading ">" switches to commands, and is erased; without it, a command
    // is found too, but Enter still creates the note of that name.
    cx.simulate_keystrokes("secondary-z secondary-a secondary-p");
    cx.simulate_input("italique");
    assert_eq!(runs(cx), ["editor::Italic"]);
    cx.simulate_keystrokes("home");
    cx.simulate_input(">");
    assert_eq!(mode(cx), Some((true, "Note")));
    assert_eq!(shell.read_with(cx, |s, cx| s.palette.as_ref().unwrap().read(cx).listed()), 1);
    cx.simulate_keystrokes("enter");
    assert_eq!(text(cx), "*mot*");

    // On a CSV table, opened from the palette: it stays shown and takes input. Help and the menu
    // start with its keys, the empty palette offers its gestures.
    shell.update(cx, |s, cx| {
        let (notes, dirs, images) = vault::scan(&root);
        s.sync(notes, dirs, images, cx);
    });
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("data.csv");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(shell.update_in(cx, |s, window, cx| s.shown_sheet().is_some_and(|sheet| sheet.focus_handle(cx).is_focused(window))));
    let sheet = shell.read_with(cx, |s, _| s.shown_sheet().unwrap());
    // The command palette starts with the table's.
    cx.simulate_keystrokes("f10");
    assert_eq!(mode(cx), Some((true, "Tableau CSV")));
    let first = runs(cx)[0];
    assert!(first.starts_with("editor::Table") || first.starts_with("sheet::"), "{first}");
    cx.simulate_keystrokes("escape f1");
    assert_eq!(shell.read_with(cx, |s, cx| s.help_rows(cx)[0].0), "Tableaux CSV");
    // Closing help, or the palette without choosing anything, gives input back to the table, which stays shown.
    cx.simulate_keystrokes("f1");
    cx.run_until_parked();
    let on_sheet = |cx: &mut gpui::VisualTestContext| shell.update_in(cx, |s, window, cx| s.shown_sheet().is_some_and(|sheet| sheet.focus_handle(cx).is_focused(window)));
    assert!(on_sheet(cx));
    cx.simulate_keystrokes("secondary-p");
    assert!(runs(cx).contains(&"editor::TableRowAbove"), "{:?}", runs(cx));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(on_sheet(cx));
    // A whole column chosen: the table's bar shows its gestures, a click runs one.
    sheet.update(cx, |sheet, cx| sheet.pick("col", 0, cx));
    cx.run_until_parked();
    // A pointer move makes the test window redraw.
    cx.simulate_mouse_move(gpui::point(px(400.), px(300.)), None, gpui::Modifiers::none());
    let at = cx.debug_bounds("sheet-span-editor::TableColRight").expect("le geste est dans la barre").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(sheet.read_with(cx, |sheet, _| sheet.table().cols()), 3);
    sheet.update(cx, |sheet, cx| sheet.pick("row", 1, cx));
    cx.run_until_parked();
    // A pointer move makes the test window redraw.
    cx.simulate_mouse_move(gpui::point(px(400.), px(300.)), None, gpui::Modifiers::none());
    let at = cx.debug_bounds("sheet-span-sheet::DeleteRows").expect("une ligne entière a ses gestes").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(sheet.read_with(cx, |sheet, _| (sheet.table().rows(), sheet.table().cell(1, 0).to_string())), (2, "Bob".to_string()));
    fs::remove_dir_all(&root).unwrap();
}

/// Block handles, as in Notion: hovering a line, dragging the handle moves its block,
/// a click opens its menu, the "+" puts a block below with the `/` list; Alt+arrow moves it.
#[gpui::test]
fn block_handles(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-blocks-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx.run_until_parked();
    let note = "# Titre\n\nUn\n\nDeux\n\n- a\n    - sous\n- b\n";
    let text = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string());
    let load = |cx: &mut gpui::VisualTestContext, text: &str| {
        shell.update_in(cx, |s, window, cx| {
            s.pane().editor.update(cx, |e, cx| e.load(text.to_string(), 0, cx));
            window.focus(&s.pane().editor.focus_handle(cx));
        });
        cx.run_until_parked();
    };
    load(cx, note);
    let none = gpui::Modifiers::none();
    let spot = |cx: &mut gpui::VisualTestContext, needle: &str| {
        let at = text(cx).find(needle).unwrap();
        shell.read_with(cx, |s, cx| s.pane().editor.read(cx).point_of(at).unwrap()) + gpui::point(px(3.), px(0.))
    };
    let hover = |cx: &mut gpui::VisualTestContext, needle: &str| {
        let at = spot(cx, needle);
        cx.simulate_mouse_move(at, None, none);
        shell.read_with(cx, |s, cx| s.pane().editor.read(cx).hovered_block().map(str::to_string))
    };
    let handle = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).block_handle()).expect("la poignée est là");

    // Hovering a line shows its block's handle: an item carries its sub-items. The
    // title, which names the note, has none.
    assert_eq!(hover(cx, "Titre"), None);
    assert_eq!(hover(cx, "Un").as_deref(), Some("Un"));
    assert_eq!(hover(cx, "- a").as_deref(), Some("- a\n    - sous"));
    assert_eq!(hover(cx, "sous").as_deref(), Some("    - sous"));
    // A sub-item's handle is at its indent.
    let inner = handle(cx).0;
    hover(cx, "- b");
    assert!(inner.left() > handle(cx).0.left() + px(10.), "{inner:?}");
    // Dragged to the top of another line, the handle drops the block there, with its blank lines.
    hover(cx, "Un");
    let (grip, plus) = handle(cx);
    assert!(plus.is_some_and(|plus| plus.right() <= grip.left()));
    let to = spot(cx, "- a") - gpui::point(px(0.), px(8.));
    cx.simulate_mouse_down(grip.center(), MouseButton::Left, none);
    cx.simulate_mouse_move(grip.center() + gpui::point(px(0.), px(10.)), MouseButton::Left, none);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    cx.simulate_mouse_up(to, MouseButton::Left, none);
    assert_eq!(text(cx), "# Titre\n\nDeux\n\nUn\n\n- a\n    - sous\n- b\n");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), note);
    // An item drops among the others, without a blank line.
    hover(cx, "- b");
    let (grip, _) = handle(cx);
    let to = spot(cx, "- a") - gpui::point(px(0.), px(8.));
    cx.simulate_mouse_down(grip.center(), MouseButton::Left, none);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    cx.simulate_mouse_up(to, MouseButton::Left, none);
    assert_eq!(text(cx), "# Titre\n\nUn\n\nDeux\n\n- b\n- a\n    - sous\n");
    cx.simulate_keystrokes("secondary-z");

    // A click on the handle takes the block and opens its menu; "Titre 2" turns it into a heading.
    hover(cx, "Deux");
    let (grip, _) = handle(cx);
    cx.simulate_click(grip.center(), none);
    assert!(shell.read_with(cx, |s, _| s.menu.as_ref().is_some_and(|m| m.block)));
    assert_eq!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).selected().to_string()), "Deux");
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Block(editor::BlockOp::Turn(block::Turn::Heading(2))), None, window, cx));
    assert_eq!(text(cx), "# Titre\n\nUn\n\n## Deux\n\n- a\n    - sous\n- b\n");
    // The same block, still targeted: duplicated, then deleted.
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Block(editor::BlockOp::Duplicate), None, window, cx));
    assert_eq!(text(cx), "# Titre\n\nUn\n\n## Deux\n\n## Deux\n\n- a\n    - sous\n- b\n");
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Block(editor::BlockOp::Delete), None, window, cx));
    cx.simulate_keystrokes("secondary-z secondary-z secondary-z");
    assert_eq!(text(cx), note);

    // Alt+Down moves the cursor's block down one block, Alt+Up moves it back up; an item, among items.
    load(cx, note);
    let place = |cx: &mut gpui::VisualTestContext, needle: &str| {
        let at = note.find(needle).unwrap();
        shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.place_cursor(at, cx)));
    };
    place(cx, "Un");
    cx.simulate_keystrokes("alt-down");
    assert_eq!(text(cx), "# Titre\n\nDeux\n\nUn\n\n- a\n    - sous\n- b\n");
    cx.simulate_keystrokes("alt-up");
    assert_eq!(text(cx), note);
    cx.simulate_keystrokes("alt-up");
    assert_eq!(text(cx), note, "rien ne monte au-dessus du titre");
    place(cx, "- a");
    cx.simulate_keystrokes("alt-down");
    assert_eq!(text(cx), "# Titre\n\nUn\n\nDeux\n\n- b\n- a\n    - sous\n");

    // The "+" puts a line below the block, spaced like its neighbours, and opens the `/` list.
    load(cx, note);
    hover(cx, "Un");
    let (_, plus) = handle(cx);
    cx.simulate_click(plus.unwrap().center(), none);
    assert_eq!(text(cx), "# Titre\n\nUn\n\n/\n\nDeux\n\n- a\n    - sous\n- b\n");
    cx.simulate_input("h1");
    cx.simulate_keystrokes("enter");
    assert!(text(cx).starts_with("# Titre\n\nUn\n\n# "), "{}", text(cx));
    fs::remove_dir_all(&root).unwrap();
}

/// Templates end to end, and their limits: no folder, a name taken or unreadable, a template
/// that is not one, a link that must not target a template, the daily note, undo.
#[gpui::test]
fn templates_journey(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-tpl-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("Courses.md"), "# Courses\n\n- lait #maison\n").unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx.run_until_parked();
    let text = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string());
    let told = |cx: &mut gpui::VisualTestContext, tone, part: &str| shell.read_with(cx, |s, _| s.told(tone, part));
    let palette = |cx: &mut gpui::VisualTestContext, typed: &str| {
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input(typed);
        cx.simulate_keystrokes("down enter");
    };
    // Once the palette is proven, gestures are launched by their action: the palette's list
    // changes with the notes that resemble what is typed.
    let start = |cx: &mut gpui::VisualTestContext, act: TemplateAct| {
        shell.update_in(cx, |s, window, cx| if act == TemplateAct::Save { s.save_as_template(window, cx) } else { s.open_templates(act, window, cx) });
    };
    let rescan = |cx: &mut gpui::VisualTestContext| {
        shell.update(cx, |s, cx| {
            let (notes, dirs, images) = vault::scan(&root);
            s.sync(notes, dirs, images, cx);
        });
        cx.run_until_parked();
    };
    // Without a templates folder: the command says so, nothing is created.
    palette(cx, "nouvelle note depuis");
    assert!(told(cx, Tone::Info, "Aucun modèle"));
    assert!(!root.join("Templates").exists());
    // Two templates, one deeper in the folder; their `#tags` and `[[links]]` belong to no one.
    fs::create_dir_all(root.join("Modèles/Sous")).unwrap();
    fs::write(root.join("Modèles/Réunion.md"), "# {{title}}\n\nLe {{date}} à {{time}} #modele [[{{title}}]]\n\n## Notes\n\n{{cursor}}\n").unwrap();
    fs::write(root.join("Modèles/Sous/Idée.md"), "idée {{inconnu}}").unwrap();
    rescan(cx);
    let kept = shell.read_with(cx, |s, _| s.notes.iter().filter(|n| n.name == "Réunion" || n.name == "Idée").map(|n| (n.tags.len(), n.links.len())).collect::<Vec<_>>());
    assert_eq!(kept, [(0, 0), (0, 0)]);
    assert!(shell.read_with(cx, |s, _| s.wiki_path("Réunion").is_none() && s.wiki_path("modele").is_none()), "un lien ne vise pas un modèle");
    // A note is born from the template, with the chosen name: title, date, time, cursor at its mark.
    palette(cx, "nouvelle note depuis");
    cx.simulate_input("Réu");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("Lundi 3");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let made = shell.read_with(cx, |s, _| s.pane().path.clone().unwrap());
    assert_eq!(made.file_name().unwrap(), "Lundi 3.md");
    let day = date_name(today());
    let body = fs::read_to_string(&made).unwrap();
    let stamp = body.split(&format!("Le {day} à ")).nth(1).map(|rest| rest[..5].to_string()).unwrap_or_default();
    assert!(stamp.len() == 5 && stamp.as_bytes()[2] == b':' && stamp.bytes().filter(u8::is_ascii_digit).count() == 4, "heure : {stamp:?} dans {body}");
    assert!(body.starts_with("# Lundi 3\n\nLe ") && body.contains("[[Lundi 3]]") && !body.contains("{{"), "{body}");
    assert_eq!(text(cx), body);
    assert_eq!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).position()), body.len() - 1, "le curseur est à la marque {{cursor}}");
    // The keyboard goes to the editor: one can type right away, at the mark.
    cx.simulate_input("!");
    assert!(text(cx).ends_with("## Notes\n\n!\n"), "{}", text(cx));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), body);
    // The name is taken: refused, the existing note is intact. An empty name does nothing; a name with
    // forbidden characters is cleaned.
    fs::write(&made, "# Lundi 3\n\nécrit à la main\n").unwrap();
    start(cx, TemplateAct::New);
    cx.simulate_input("Réu");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("Lundi 3");
    cx.simulate_keystrokes("enter");
    assert!(told(cx, Tone::Failed, "porte déjà le nom"));
    assert_eq!(fs::read_to_string(&made).unwrap(), "# Lundi 3\n\nécrit à la main\n");
    let files = |root: &Path| fs::read_dir(root).unwrap().count();
    let before = files(&root);
    start(cx, TemplateAct::New);
    cx.simulate_input("Idée");
    cx.simulate_keystrokes("enter enter");
    assert_eq!(files(&root), before, "un nom vide ne crée rien");
    start(cx, TemplateAct::New);
    cx.simulate_input("Idée");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("a/b:c");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(root.join("abc.md")).unwrap(), "idée {{inconnu}}");
    // An unreadable template (bytes that are not text): a notice, nothing is born; an empty template: an empty note.
    fs::write(root.join("Modèles/Cassé.md"), [0xff, 0xfe, 0x00]).unwrap();
    fs::write(root.join("Modèles/Vide.md"), "").unwrap();
    rescan(cx);
    start(cx, TemplateAct::New);
    cx.simulate_input("Cassé");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("Rien");
    cx.simulate_keystrokes("enter");
    assert!(told(cx, Tone::Failed, "Modèle illisible") && !root.join("Rien.md").exists());
    start(cx, TemplateAct::New);
    cx.simulate_input("Vide");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("Blanc");
    cx.simulate_keystrokes("enter");
    assert_eq!(fs::read(root.join("Blanc.md")).unwrap(), b"");
    // Insert at the cursor: filled with the open note's name, a single undo step.
    shell.update(cx, |s, cx| s.open_note(&made, cx));
    let before = text(cx);
    shell.update_in(cx, |s, window, cx| window.focus(&s.pane().editor.focus_handle(cx)));
    cx.simulate_keystrokes("secondary-end");
    start(cx, TemplateAct::Insert);
    cx.simulate_input("Réu");
    cx.simulate_keystrokes("enter");
    assert!(text(cx).starts_with(&format!("{before}# Lundi 3\n\nLe ")), "{}", text(cx));
    assert_eq!(text(cx).matches("[[Lundi 3]]").count(), before.matches("[[Lundi 3]]").count() + 1);
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), before);
    // Make a template from the note: its title becomes {{title}}, the existing folder is reused.
    start(cx, TemplateAct::Save);
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_input("Copie");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(root.join("Modèles/Copie.md")).unwrap(), markdown::templatize(&before));
    assert!(fs::read_to_string(root.join("Modèles/Copie.md")).unwrap().starts_with("# {{title}}\n"));
    assert!(!root.join("Templates").exists());
    // The daily note is born from the `Daily` template when it exists; a second Ctrl+J reopens the same note.
    let name = date_name(today());
    if let Some(old) = shell.read_with(cx, |s, _| s.wiki_path(&name)) {
        fs::remove_file(old).unwrap();
    }
    rescan(cx);
    fs::write(root.join("Modèles/Daily.md"), "# {{title}}\n\n- [ ] {{cursor}}").unwrap();
    rescan(cx);
    cx.simulate_keystrokes("secondary-j");
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| s.pane().path.clone().unwrap().file_name().unwrap().to_string_lossy().into_owned()), format!("{name}.md"));
    assert_eq!(text(cx), format!("# {name}\n\n- [ ] "));
    cx.simulate_keystrokes("secondary-j");
    assert_eq!(text(cx), format!("# {name}\n\n- [ ] "));
    let _ = fs::remove_dir_all(&root);
}

/// Text formatting: bold, italic, underline, strikethrough, highlight, code, by keyboard and by
/// the bottom bar, with their limits (cursor alone, blanks, several lines, code, Backspace).
#[gpui::test]
fn formatting_journey(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-fmt-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("Note.md"), "x").unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx.simulate_resize(size(px(900.), px(600.)));
    cx.run_until_parked();
    let text = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string());
    // The text loaded, then `len` characters selected from `from` (in characters).
    let pick = |cx: &mut gpui::VisualTestContext, note: &str, from: usize, len: usize| {
        let at = note.char_indices().nth(from).map_or(note.len(), |(i, _)| i);
        shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.load(note.to_string(), at, cx)));
        cx.run_until_parked();
        if len > 0 {
            cx.simulate_keystrokes(&"shift-right ".repeat(len));
        }
    };
    let selected = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).selected_text());
    let styles = |cx: &mut gpui::VisualTestContext| shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, _| e.styles()));
    // Each style: its marks around the word, the selection stays on the word, the same gesture removes them.
    for (key, shown, flag) in [
        ("secondary-b", "un **mot** ici", markdown::BOLD),
        ("secondary-i", "un *mot* ici", markdown::ITALIC),
        ("secondary-u", "un <u>mot</u> ici", markdown::UNDER),
        ("secondary-shift-x", "un ~~mot~~ ici", markdown::STRIKE),
        ("secondary-shift-h", "un ==mot== ici", markdown::HIGHLIGHT),
        ("secondary-shift-k", "un `mot` ici", markdown::CODE),
    ] {
        pick(cx, "un mot ici", 3, 3);
        assert_eq!(styles(cx), 0, "{key}");
        cx.simulate_keystrokes(key);
        assert_eq!((text(cx), selected(cx).as_str()), (shown.to_string(), "mot"), "{key}");
        assert_eq!(styles(cx), flag, "{key} : le bouton s'allume");
        cx.simulate_keystrokes(key);
        assert_eq!((text(cx), selected(cx).as_str()), ("un mot ici".to_string(), "mot"), "{key} retiré");
        // A single Backspace undoes the insertion; redo puts it back.
        cx.simulate_keystrokes(key);
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(text(cx), "un mot ici", "{key} annulé");
        cx.simulate_keystrokes("secondary-shift-z");
        assert_eq!(text(cx), shown, "{key} rétabli");
    }
    // Several styles on a word, then one in the middle is removed: the others stay.
    pick(cx, "un mot ici", 3, 3);
    cx.simulate_keystrokes("secondary-b secondary-u secondary-shift-h");
    assert_eq!(text(cx), "un **<u>==mot==</u>** ici");
    assert_eq!(styles(cx), markdown::BOLD | markdown::UNDER | markdown::HIGHLIGHT);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "un <u>==mot==</u> ici");
    cx.simulate_keystrokes("secondary-u");
    assert_eq!((text(cx), selected(cx).as_str()), ("un ==mot== ici".to_string(), "mot"));
    // Bold around italic: the cursor in the italic removes the bold without touching the italic.
    pick(cx, "**a *b* c**", 5, 0);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "a *b* c");
    // Cursor alone: two marks and the cursor between them; typing fills them; the next gesture removes them.
    pick(cx, "ab", 1, 0);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "a****b");
    cx.simulate_input("x");
    assert_eq!(text(cx), "a**x**b");
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "axb");
    // Empty pair: the gesture removes it instead of stacking another.
    pick(cx, "ab", 1, 0);
    cx.simulate_keystrokes("secondary-b secondary-b");
    assert_eq!(text(cx), "ab");
    // Edge blanks stay outside, the bullet and the heading too.
    pick(cx, "un mot ici", 3, 4);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "un **mot** ici");
    pick(cx, "- liste", 0, 7);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "- **liste**");
    pick(cx, "# Titre", 2, 5);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "# **Titre**");
    // Only blanks: nothing moves, and nothing to undo.
    pick(cx, "a   b", 1, 3);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "a   b");
    // Text in other scripts, accents, emojis, wide characters: the marks land on the right bytes.
    for note in ["日本語 é🙂", "é", "🙂", "a|b\\c \"d\""] {
        pick(cx, note, 0, note.chars().count());
        cx.simulate_keystrokes("secondary-b secondary-shift-h");
        assert_eq!(text(cx), format!("**=={note}==**"), "{note}");
        cx.simulate_keystrokes("secondary-shift-h secondary-b");
        assert_eq!(text(cx), note, "{note}");
    }
    // Several lines: each has its marks, except code blocks and blank lines; one gesture, one undo.
    let note = "- un\n- deux\n\n```\ncode\n```\nfin";
    pick(cx, note, 0, note.chars().count());
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "- **un**\n- **deux**\n\n```\ncode\n```\n**fin**");
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), note);
    cx.simulate_keystrokes("secondary-b secondary-z");
    assert_eq!(text(cx), note);
    // An unclosed code block is not one: its text gets the style.
    pick(cx, "```\nsuite", 4, 5);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "```\n**suite**");
    // Inside a code block, nothing changes.
    pick(cx, "```\ncode\n```", 5, 4);
    cx.simulate_keystrokes("secondary-b secondary-u");
    assert_eq!(text(cx), "```\ncode\n```");
    pick(cx, "```\ncode\n```", 6, 0);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "```\ncode\n```");
    // A table cell, and a comment: the selection stays valid there.
    pick(cx, "| a | b |\n| --- | --- |\n| c | d |", 26, 1);
    cx.simulate_keystrokes("secondary-b");
    assert!(text(cx).contains("| **c** |"), "{}", text(cx));
    // Several cursors: each gets its marks, a single undo removes them all.
    pick(cx, "un chat\nun chien", 0, 0);
    cx.simulate_keystrokes("secondary-d secondary-d secondary-u");
    assert_eq!(text(cx), "<u>un</u> chat\n<u>un</u> chien");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), "un chat\nun chien");
    cx.simulate_keystrokes("escape");

    // The bottom bar: in the middle of the page, left of the counter, eleven buttons of the same size.
    pick(cx, "un mot ici", 3, 3);
    let (bar, counts, pane) = (cx.debug_bounds("format-bar").unwrap(), cx.debug_bounds("counts").unwrap(), cx.debug_bounds("pane-0").unwrap());
    assert!(bar.right() <= counts.left() && bar.left() >= pane.left() && bar.bottom() <= pane.bottom(), "{bar:?} {counts:?} {pane:?}");
    // In the middle of the page, or a bit to the left when the counter (long, with a selection) pushes it.
    assert!(bar.center().x <= pane.center().x + px(1.) && bar.center().x >= pane.center().x - px(60.), "{bar:?} {pane:?}");
    assert_eq!((bar.size.height, bar.size.width), (counts.size.height, px(335.)));
    const NAMES: [&str; 11] = ["format-0", "format-1", "format-2", "format-3", "format-4", "format-5", "format-6", "format-7", "format-8", "format-9", "format-10"];
    let buttons = |cx: &mut gpui::VisualTestContext| NAMES.iter().map(|name| cx.debug_bounds(name).unwrap_or_else(|| panic!("{name} absent"))).collect::<Vec<_>>();
    let all = buttons(cx);
    assert!(all.iter().all(|b| b.size == all[0].size && b.top() == all[0].top() && bar.contains(&b.center())), "{all:?}");
    assert!(all.windows(2).all(|w| w[0].right() <= w[1].left()), "{all:?}");
    // A click: the style is applied to the selection, which stays, and the button lights up; a second click removes it.
    let click = |cx: &mut gpui::VisualTestContext, i: usize| {
        let at = cx.debug_bounds(NAMES[i]).unwrap().center();
        cx.simulate_click(at, gpui::Modifiers::none());
    };
    click(cx, 0);
    assert_eq!((text(cx), selected(cx).as_str()), ("un **mot** ici".to_string(), "mot"));
    assert_eq!(styles(cx), markdown::BOLD);
    click(cx, 2);
    click(cx, 4);
    assert_eq!((text(cx), selected(cx).as_str()), ("un **<u>==mot==</u>** ici".to_string(), "mot"));
    click(cx, 0);
    click(cx, 2);
    click(cx, 4);
    assert_eq!(text(cx), "un mot ici");
    click(cx, 3);
    click(cx, 5);
    assert_eq!(text(cx), "un ~~`mot`~~ ici");
    // Colours: the chevron opens the badges, a badge highlights in its colour (`<mark>`),
    // the same one a second time removes it, another changes the colour; H removes whichever it is; Escape closes.
    pick(cx, "un mot ici", 3, 3);
    let open_swatches = |cx: &mut gpui::VisualTestContext| {
        let at = cx.debug_bounds("format-colors").unwrap().center();
        cx.simulate_click(at, gpui::Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    };
    let swatch = |cx: &mut gpui::VisualTestContext, i: usize| {
        const SWATCHES: [&str; 5] = ["swatch-0", "swatch-1", "swatch-2", "swatch-3", "swatch-4"];
        let at = cx.debug_bounds(SWATCHES[i]).unwrap().center();
        cx.simulate_click(at, gpui::Modifiers::none());
    };
    open_swatches(cx);
    let (bar, pops) = (cx.debug_bounds("format-bar").unwrap(), cx.debug_bounds("format-swatches").unwrap());
    assert!(pops.bottom() <= bar.top() && pops.left() >= bar.left() && pops.right() <= bar.right() + px(120.), "{pops:?} {bar:?}");
    swatch(cx, 1);
    assert_eq!((text(cx), selected(cx).as_str()), ("un <mark class=\"green\">mot</mark> ici".to_string(), "mot"));
    assert_eq!(styles(cx), markdown::HIGHLIGHT);
    open_swatches(cx);
    swatch(cx, 3);
    assert_eq!(text(cx), "un <mark class=\"pink\">mot</mark> ici");
    open_swatches(cx);
    swatch(cx, 3);
    assert_eq!((text(cx), selected(cx).as_str()), ("un mot ici".to_string(), "mot"));
    open_swatches(cx);
    swatch(cx, 0);
    assert_eq!(text(cx), "un ==mot== ici");
    open_swatches(cx);
    swatch(cx, 4);
    click(cx, 4);
    assert_eq!(text(cx), "un mot ici");
    // On several lines, and the gesture is undone at once; Escape closes the badges without changing anything.
    pick(cx, "- a\n- b", 0, 7);
    open_swatches(cx);
    swatch(cx, 2);
    assert_eq!(text(cx), "- <mark class=\"blue\">a</mark>\n- <mark class=\"blue\">b</mark>");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), "- a\n- b");
    open_swatches(cx);
    cx.simulate_keystrokes("escape");
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, cx| !s.pane().editor.read(cx).swatches_open()));
    assert_eq!(text(cx), "- a\n- b");
    // The link from the bar: the selection becomes the text, the cursor waits for the address.
    pick(cx, "un mot ici", 3, 3);
    click(cx, 6);
    assert_eq!(text(cx), "un [mot]() ici");
    // Lists: a click changes the line's kind, the same button turns it back into text, another changes the kind.
    pick(cx, "un mot ici\ndeux", 0, 14);
    click(cx, 7);
    assert_eq!(text(cx), "- un mot ici\n- deux");
    click(cx, 7);
    assert_eq!(text(cx), "un mot ici\ndeux");
    click(cx, 8);
    assert_eq!(text(cx), "1. un mot ici\n2. deux");
    click(cx, 9);
    assert_eq!(text(cx), "- [ ] un mot ici\n- [ ] deux");
    click(cx, 7);
    assert_eq!(text(cx), "- un mot ici\n- deux");
    cx.simulate_keystrokes("secondary-z secondary-z secondary-z secondary-z secondary-z");
    assert_eq!(text(cx), "un mot ici\ndeux");
    // The table: the button opens the size grid; Enter inserts it. Inside the table, lists give way to its rows and columns.
    pick(cx, "avant", 5, 0);
    click(cx, 10);
    cx.simulate_keystrokes("enter");
    assert!(text(cx).contains("|  |  |  |\n| --- | --- | --- |") || text(cx).contains("| --- |"), "{}", text(cx));
    assert!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).in_table()));
    cx.run_until_parked();
    let rows = |cx: &mut gpui::VisualTestContext| text(cx).lines().filter(|l| l.starts_with('|')).count();
    let columns = |cx: &mut gpui::VisualTestContext| text(cx).lines().find(|l| l.starts_with('|')).unwrap().matches('|').count() - 1;
    let (r, c) = (rows(cx), columns(cx));
    click(cx, 7);
    assert_eq!((rows(cx), columns(cx)), (r + 1, c), "{:?}", text(cx));
    click(cx, 8);
    assert_eq!((rows(cx), columns(cx)), (r + 1, c + 1));
    click(cx, 9);
    click(cx, 10);
    assert_eq!((rows(cx), columns(cx)), (r, c));
    let all = buttons(cx);
    assert_eq!(all.len(), 11);
    // A single undo gesture for each.
    cx.simulate_keystrokes("secondary-z");
    assert_eq!((rows(cx), columns(cx)), (r, c + 1));
    // The bar, at any width, never disappears: in the middle of the page while there is room, then further
    // left, then folded into a button at the bottom left, which opens it above itself (its buttons wrap).
    let mut seen = Vec::new();
    for width in [1400., 900., 700., 600., 560., 480., 420., 320., 240.] {
        cx.simulate_resize(size(px(width), px(600.)));
        cx.run_until_parked();
        for _ in 0..2 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
        // The bounds of a vanished element stay in memory in the test: the editor is what says what is on screen.
        let (shown, folded) = shell.read_with(cx, |s, cx| (s.pane().editor.read(cx).bar_shown(), s.pane().editor.read(cx).bar_folded()));
        let (counts, pane) = (cx.debug_bounds("counts").unwrap(), cx.debug_bounds("pane-0").unwrap());
        assert!(counts.left() >= pane.left() && counts.right() <= pane.right(), "{width} : {counts:?} {pane:?}");
        assert!(shown != folded, "{width} : la barre est là, ou repliée en un bouton : {shown} {folded}");
        if shown {
            let bar = cx.debug_bounds("format-bar").unwrap();
            assert!(bar.right() <= counts.left() && bar.left() >= pane.left() + px(8.) - px(0.5), "{width} : {bar:?} {counts:?} {pane:?}");
            if width >= 1400. {
                assert!((bar.center().x - pane.center().x).abs() <= px(1.), "{width} : au milieu de la page {bar:?} {pane:?}");
            }
        } else {
            // Folded: the button is in the note, at the left edge, and the counter does not go under it.
            let toggle = cx.debug_bounds("format-toggle").unwrap();
            assert!(toggle.left() >= pane.left() && toggle.right() <= counts.left().max(toggle.right()) && toggle.bottom() <= pane.bottom(), "{width} : {toggle:?} {counts:?} {pane:?}");
            assert!(counts.left() >= toggle.right() || counts.size.width == px(0.), "{width} : le compteur passe sous le bouton {toggle:?} {counts:?}");
            // Open: all buttons are in the note, wrapped if it is narrow, above the button; a click acts.
            pick(cx, "un mot ici", 3, 3);
            let at = cx.debug_bounds("format-toggle").unwrap().center();
            cx.simulate_click(at, gpui::Modifiers::none());
            cx.run_until_parked();
            for _ in 0..2 {
                cx.update(|window, _| window.refresh());
                cx.run_until_parked();
            }
            assert!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).bar_shown()), "{width} : le bouton ouvre la barre");
            let (bar, toggle) = (cx.debug_bounds("format-bar").unwrap(), cx.debug_bounds("format-toggle").unwrap());
            assert!(bar.left() >= pane.left() && bar.right() <= pane.right() && bar.bottom() <= toggle.top() && bar.top() >= pane.top(), "{width} : {bar:?} {toggle:?} {pane:?}");
            for name in NAMES {
                let button = cx.debug_bounds(name).unwrap();
                assert!(bar.contains(&button.center()) && button.left() >= bar.left() && button.right() <= bar.right(), "{width} : {name} {button:?} hors de {bar:?}");
            }
            click(cx, 0);
            assert_eq!((text(cx), selected(cx).as_str()), ("un **mot** ici".to_string(), "mot"), "{width}");
            click(cx, 0);
            assert_eq!(text(cx), "un mot ici", "{width}");
            // The colour badges open too, above the bar and inside the note.
            let chevron = cx.debug_bounds("format-colors").unwrap().center();
            cx.simulate_click(chevron, gpui::Modifiers::none());
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
            let (pops, bar) = (cx.debug_bounds("format-swatches").unwrap(), cx.debug_bounds("format-bar").unwrap());
            assert!(pops.left() >= pane.left() && pops.right() <= pane.right() && pops.bottom() <= bar.top(), "{width} : {pops:?} {bar:?} {pane:?}");
            let green = cx.debug_bounds("swatch-1").unwrap().center();
            cx.simulate_click(green, gpui::Modifiers::none());
            assert_eq!(text(cx), "un <mark class=\"green\">mot</mark> ici", "{width}");
            click(cx, 4);
            assert_eq!(text(cx), "un mot ici", "{width}");
            // Escape closes it without touching the text.
            cx.simulate_keystrokes("escape");
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
            assert!(shell.read_with(cx, |s, cx| !s.pane().editor.read(cx).bar_shown() && s.pane().editor.read(cx).bar_folded()), "{width}");
            assert_eq!(text(cx), "un mot ici");
        }
        seen.push(shown);
    }
    assert!(seen[0] && !seen[8] && seen.windows(2).all(|w| w[0] >= w[1]), "elle se replie d'un seul bord, sans revenir en arrière : {seen:?}");
    // Two panes: a single bar, the one of the pane being typed in.
    cx.simulate_resize(size(px(1400.), px(600.)));
    cx.run_until_parked();
    cx.simulate_keystrokes("secondary-\\");
    cx.run_until_parked();
    for _ in 0..2 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    let bars = shell.read_with(cx, |s, cx| s.panes.iter().filter(|p| p.editor.read(cx).bar_shown()).count());
    assert_eq!(bars, 1, "une seule barre pour deux panes");
    cx.dispatch_action(ClosePane);
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| s.panes.len()), 1);
    cx.simulate_resize(size(px(900.), px(600.)));
    cx.run_until_parked();
    // Setting « Barre de mise en forme… »: hidden, it is no longer drawn and the choice survives a restart;
    // an empty, missing or unknown value leaves it shown.
    let choose = |cx: &mut gpui::VisualTestContext, keys: &str| {
        shell.update_in(cx, |s, window, cx| s.choose_setting(Setting::Bar, window, cx));
        cx.run_until_parked();
        cx.simulate_keystrokes(keys);
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    };
    choose(cx, "down enter");
    assert!(shell.read_with(cx, |s, _| s.prefs.bar == "off" && !s.theme.bar));
    assert!(!shell.read_with(cx, |s, cx| s.pane().editor.read(cx).bar_shown()) && cx.debug_bounds("counts").is_some());
    assert!(vault::load_settings().contains("bar=off\n"));
    // Escape during the list: the previewed choice is taken back.
    choose(cx, "up escape");
    assert!(shell.read_with(cx, |s, _| s.prefs.bar == "off"));
    choose(cx, "up enter");
    assert!(shell.read_with(cx, |s, cx| s.theme.bar && s.pane().editor.read(cx).bar_shown()));
    for (text, shown) in [("", true), ("bar=\n", true), ("bar=on\n", true), ("bar=zzz\n", true), ("bar=off\n", false)] {
        assert_eq!(Prefs::parse(text).bar != "off", shown, "{text:?}");
    }
    assert_eq!(Prefs::parse(&Prefs::parse("bar=off").to_text()).bar, "off");
    assert!(!Prefs::parse("").to_text().contains("bar="));
    let _ = fs::remove_dir_all(&root);
}
