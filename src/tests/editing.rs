//! Editing journeys: `/` commands, block handles, inline formatting and its bar, templates.

use super::*;

/// Ce qu'on peut faire se trouve sans le savoir : la palette des commandes (F10, ou « > »), qui les
/// cherche par ce qu'elles font, celles de la vue d'abord ; l'aide ouverte sur la vue ; la barre d'un tableau.
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
    let text = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).text().to_string());
    shell.update_in(cx, |s, window, cx| window.focus(&s.editor.focus_handle(cx)));
    cx.simulate_input("mot");
    let runs = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.palette.as_ref().map(|p| p.read(cx).runs()).unwrap_or_default());
    let mode = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.palette.as_ref().map(|p| p.read(cx).mode()));

    // F10 ouvre la palette des commandes : rien que les commandes que la note sait faire (pas celles
    // d'une grille), sous le nom de ce qu'on regarde.
    cx.simulate_keystrokes("f10");
    assert_eq!(mode(cx), Some((true, "Note")));
    let all = runs(cx);
    assert!(all.contains(&"app::NewNote") && all.contains(&"editor::Bold"), "{all:?}");
    assert!(!all.contains(&"sheet::Copy") && !all.contains(&"canvas::Erase"));
    // Échap la referme et rend la saisie à la note.
    cx.simulate_keystrokes("escape");
    assert_eq!(mode(cx), None);
    assert!(shell.update_in(cx, |s, window, cx| s.editor.focus_handle(cx).is_focused(window)));
    // Taper filtre par ce que fait la commande ; Entrée la fait, sur la note.
    cx.simulate_keystrokes("secondary-a f10");
    cx.simulate_input("gras");
    assert_eq!(runs(cx), ["editor::Bold"]);
    cx.simulate_keystrokes("enter");
    assert_eq!(mode(cx), None);
    assert_eq!(text(cx), "**mot**");
    // Le premier bouton du rail l'ouvre aussi ; un clic sur une commande la fait.
    let at = cx.debug_bounds("nav-commands").expect("le bouton du rail").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(mode(cx).map(|m| m.0), Some(true));
    cx.simulate_input("raccourcis");
    let at = cx.debug_bounds("command-app::ToggleHelp").expect("la commande est affichée").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert!(shell.read_with(cx, |s, _| s.palette.is_none() && s.help));
    shell.update_in(cx, |s, window, cx| s.set_help(false, window, cx));

    // Dans la palette des notes, « > » en tête passe aux commandes, et s'efface ; sans lui, une commande
    // se trouve aussi, mais Entrée crée encore la note de ce nom.
    cx.simulate_keystrokes("secondary-z secondary-a secondary-p");
    cx.simulate_input("italique");
    assert_eq!(runs(cx), ["editor::Italic"]);
    cx.simulate_keystrokes("home");
    cx.simulate_input(">");
    assert_eq!(mode(cx), Some((true, "Note")));
    assert_eq!(shell.read_with(cx, |s, cx| s.palette.as_ref().unwrap().read(cx).listed()), 1);
    cx.simulate_keystrokes("enter");
    assert_eq!(text(cx), "*mot*");

    // Sur un tableau CSV, ouvert depuis la palette : il reste affiché et prend la saisie. L'aide et le menu
    // commencent par ses touches, la palette vide propose ses gestes.
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
    // La palette des commandes commence par celles du tableau.
    cx.simulate_keystrokes("f10");
    assert_eq!(mode(cx), Some((true, "Tableau CSV")));
    let first = runs(cx)[0];
    assert!(first.starts_with("editor::Table") || first.starts_with("sheet::"), "{first}");
    cx.simulate_keystrokes("escape f1");
    assert_eq!(shell.read_with(cx, |s, cx| s.help_rows(cx)[0].0), "Tableaux CSV");
    // Refermer l'aide, ou la palette sans rien choisir, rend la saisie au tableau, qui reste affiché.
    cx.simulate_keystrokes("f1");
    cx.run_until_parked();
    let on_sheet = |cx: &mut gpui::VisualTestContext| shell.update_in(cx, |s, window, cx| s.shown_sheet().is_some_and(|sheet| sheet.focus_handle(cx).is_focused(window)));
    assert!(on_sheet(cx));
    cx.simulate_keystrokes("secondary-p");
    assert!(runs(cx).contains(&"editor::TableRowAbove"), "{:?}", runs(cx));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(on_sheet(cx));
    // Une colonne entière choisie : la barre du tableau montre ses gestes, un clic en fait un.
    sheet.update(cx, |sheet, cx| sheet.pick("col", 0, cx));
    cx.run_until_parked();
    // Un mouvement du pointeur fait redessiner la fenêtre de test.
    cx.simulate_mouse_move(gpui::point(px(400.), px(300.)), None, gpui::Modifiers::none());
    let at = cx.debug_bounds("sheet-span-editor::TableColRight").expect("le geste est dans la barre").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(sheet.read_with(cx, |sheet, _| sheet.table().cols()), 3);
    sheet.update(cx, |sheet, cx| sheet.pick("row", 1, cx));
    cx.run_until_parked();
    // Un mouvement du pointeur fait redessiner la fenêtre de test.
    cx.simulate_mouse_move(gpui::point(px(400.), px(300.)), None, gpui::Modifiers::none());
    let at = cx.debug_bounds("sheet-span-sheet::DeleteRows").expect("une ligne entière a ses gestes").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(sheet.read_with(cx, |sheet, _| (sheet.table().rows(), sheet.table().cell(1, 0).to_string())), (2, "Bob".to_string()));
    fs::remove_dir_all(&root).unwrap();
}

/// Les poignées de bloc, comme dans Notion : au survol d'une ligne, tirer la poignée déplace son bloc,
/// un clic ouvre son menu, le « + » pose un bloc dessous avec la liste `/` ; Alt+flèche le déplace.
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
    let text = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).text().to_string());
    let load = |cx: &mut gpui::VisualTestContext, text: &str| {
        shell.update_in(cx, |s, window, cx| {
            s.editor.update(cx, |e, cx| e.load(text.to_string(), 0, cx));
            window.focus(&s.editor.focus_handle(cx));
        });
        cx.run_until_parked();
    };
    load(cx, note);
    let none = gpui::Modifiers::none();
    let spot = |cx: &mut gpui::VisualTestContext, needle: &str| {
        let at = text(cx).find(needle).unwrap();
        shell.read_with(cx, |s, cx| s.editor.read(cx).point_of(at).unwrap()) + gpui::point(px(3.), px(0.))
    };
    let hover = |cx: &mut gpui::VisualTestContext, needle: &str| {
        let at = spot(cx, needle);
        cx.simulate_mouse_move(at, None, none);
        shell.read_with(cx, |s, cx| s.editor.read(cx).hovered_block().map(str::to_string))
    };
    let handle = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).block_handle()).expect("la poignée est là");

    // Le survol d'une ligne montre la poignée de son bloc : un item emporte ses sous-items. Le
    // titre, qui nomme la note, n'en a pas.
    assert_eq!(hover(cx, "Titre"), None);
    assert_eq!(hover(cx, "Un").as_deref(), Some("Un"));
    assert_eq!(hover(cx, "- a").as_deref(), Some("- a\n    - sous"));
    assert_eq!(hover(cx, "sous").as_deref(), Some("    - sous"));
    // La poignée d'un sous-item est à son retrait.
    let inner = handle(cx).0;
    hover(cx, "- b");
    assert!(inner.left() > handle(cx).0.left() + px(10.), "{inner:?}");
    // Tirée jusqu'au haut d'une autre ligne, la poignée y pose le bloc, avec ses lignes vides.
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
    // Un item se pose parmi les autres, sans ligne vide.
    hover(cx, "- b");
    let (grip, _) = handle(cx);
    let to = spot(cx, "- a") - gpui::point(px(0.), px(8.));
    cx.simulate_mouse_down(grip.center(), MouseButton::Left, none);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    cx.simulate_mouse_up(to, MouseButton::Left, none);
    assert_eq!(text(cx), "# Titre\n\nUn\n\nDeux\n\n- b\n- a\n    - sous\n");
    cx.simulate_keystrokes("secondary-z");

    // Un clic sur la poignée prend le bloc et ouvre son menu ; « Titre 2 » le transforme.
    hover(cx, "Deux");
    let (grip, _) = handle(cx);
    cx.simulate_click(grip.center(), none);
    assert!(shell.read_with(cx, |s, _| s.menu.as_ref().is_some_and(|m| m.block)));
    assert_eq!(shell.read_with(cx, |s, cx| s.editor.read(cx).selected().to_string()), "Deux");
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Block(editor::BlockOp::Turn(block::Turn::Heading(2))), None, window, cx));
    assert_eq!(text(cx), "# Titre\n\nUn\n\n## Deux\n\n- a\n    - sous\n- b\n");
    // Le même bloc, toujours visé : dupliqué, puis supprimé.
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Block(editor::BlockOp::Duplicate), None, window, cx));
    assert_eq!(text(cx), "# Titre\n\nUn\n\n## Deux\n\n## Deux\n\n- a\n    - sous\n- b\n");
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Block(editor::BlockOp::Delete), None, window, cx));
    cx.simulate_keystrokes("secondary-z secondary-z secondary-z");
    assert_eq!(text(cx), note);

    // Alt+Bas descend le bloc du curseur d'un bloc, Alt+Haut le remonte ; un item, parmi les items.
    load(cx, note);
    let place = |cx: &mut gpui::VisualTestContext, needle: &str| {
        let at = note.find(needle).unwrap();
        shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| e.place_cursor(at, cx)));
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

    // Le « + » pose une ligne sous le bloc, séparée comme ses voisins, et ouvre la liste `/`.
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

/// Les modèles de bout en bout, et leurs limites : pas de dossier, un nom pris ou illisible, un modèle
/// qui n'en est pas un, un lien qui ne doit pas viser un modèle, la note du jour, l'annulation.
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
    let text = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).text().to_string());
    let told = |cx: &mut gpui::VisualTestContext, tone, part: &str| shell.read_with(cx, |s, _| s.told(tone, part));
    let palette = |cx: &mut gpui::VisualTestContext, typed: &str| {
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input(typed);
        cx.simulate_keystrokes("down enter");
    };
    // Une fois la palette éprouvée, les gestes se lancent par leur action : la liste de la palette
    // change avec les notes qui ressemblent à ce qu'on tape.
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
    // Sans dossier de modèles : la commande le dit, rien ne se crée.
    palette(cx, "nouvelle note depuis");
    assert!(told(cx, Tone::Info, "Aucun modèle"));
    assert!(!root.join("Templates").exists());
    // Deux modèles, dont un plus bas dans le dossier ; leurs `#tags` et `[[liens]]` ne sont à personne.
    fs::create_dir_all(root.join("Modèles/Sous")).unwrap();
    fs::write(root.join("Modèles/Réunion.md"), "# {{title}}\n\nLe {{date}} à {{time}} #modele [[{{title}}]]\n\n## Notes\n\n{{cursor}}\n").unwrap();
    fs::write(root.join("Modèles/Sous/Idée.md"), "idée {{inconnu}}").unwrap();
    rescan(cx);
    let kept = shell.read_with(cx, |s, _| s.notes.iter().filter(|n| n.name == "Réunion" || n.name == "Idée").map(|n| (n.tags.len(), n.links.len())).collect::<Vec<_>>());
    assert_eq!(kept, [(0, 0), (0, 0)]);
    assert!(shell.read_with(cx, |s, _| s.wiki_path("Réunion").is_none() && s.wiki_path("modele").is_none()), "un lien ne vise pas un modèle");
    // Une note naît du modèle, au nom choisi : titre, date, heure, curseur à sa marque.
    palette(cx, "nouvelle note depuis");
    cx.simulate_input("Réu");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("Lundi 3");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let made = shell.read_with(cx, |s, _| s.path.clone().unwrap());
    assert_eq!(made.file_name().unwrap(), "Lundi 3.md");
    let day = date_name(today());
    let body = fs::read_to_string(&made).unwrap();
    let stamp = body.split(&format!("Le {day} à ")).nth(1).map(|rest| rest[..5].to_string()).unwrap_or_default();
    assert!(stamp.len() == 5 && stamp.as_bytes()[2] == b':' && stamp.bytes().filter(u8::is_ascii_digit).count() == 4, "heure : {stamp:?} dans {body}");
    assert!(body.starts_with("# Lundi 3\n\nLe ") && body.contains("[[Lundi 3]]") && !body.contains("{{"), "{body}");
    assert_eq!(text(cx), body);
    assert_eq!(shell.read_with(cx, |s, cx| s.editor.read(cx).position()), body.len() - 1, "le curseur est à la marque {{cursor}}");
    // Le clavier est à l'éditeur : on peut écrire tout de suite, à la marque.
    cx.simulate_input("!");
    assert!(text(cx).ends_with("## Notes\n\n!\n"), "{}", text(cx));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), body);
    // Le nom est pris : refusé, la note existante est intacte. Un nom vide ne fait rien ; un nom avec des
    // caractères interdits est nettoyé.
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
    // Un modèle illisible (octets qui ne sont pas du texte) : un avis, rien ne naît ; un modèle vide : une note vide.
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
    // Insérer au curseur : rempli au nom de la note ouverte, un seul pas d'annulation.
    shell.update(cx, |s, cx| s.open_note(&made, cx));
    let before = text(cx);
    shell.update_in(cx, |s, window, cx| window.focus(&s.editor.focus_handle(cx)));
    cx.simulate_keystrokes("secondary-end");
    start(cx, TemplateAct::Insert);
    cx.simulate_input("Réu");
    cx.simulate_keystrokes("enter");
    assert!(text(cx).starts_with(&format!("{before}# Lundi 3\n\nLe ")), "{}", text(cx));
    assert_eq!(text(cx).matches("[[Lundi 3]]").count(), before.matches("[[Lundi 3]]").count() + 1);
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), before);
    // Faire un modèle de la note : son titre devient {{title}}, le dossier existant est réutilisé.
    start(cx, TemplateAct::Save);
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_input("Copie");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(root.join("Modèles/Copie.md")).unwrap(), markdown::templatize(&before));
    assert!(fs::read_to_string(root.join("Modèles/Copie.md")).unwrap().starts_with("# {{title}}\n"));
    assert!(!root.join("Templates").exists());
    // La note du jour naît du modèle `Daily` quand il existe ; un second Ctrl+J rouvre la même note.
    let name = date_name(today());
    if let Some(old) = shell.read_with(cx, |s, _| s.wiki_path(&name)) {
        fs::remove_file(old).unwrap();
    }
    rescan(cx);
    fs::write(root.join("Modèles/Daily.md"), "# {{title}}\n\n- [ ] {{cursor}}").unwrap();
    rescan(cx);
    cx.simulate_keystrokes("secondary-j");
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| s.path.clone().unwrap().file_name().unwrap().to_string_lossy().into_owned()), format!("{name}.md"));
    assert_eq!(text(cx), format!("# {name}\n\n- [ ] "));
    cx.simulate_keystrokes("secondary-j");
    assert_eq!(text(cx), format!("# {name}\n\n- [ ] "));
    let _ = fs::remove_dir_all(&root);
}

/// La mise en forme du texte : gras, italique, souligné, barré, surligné, code, au clavier et par
/// la barre du bas, avec leurs limites (curseur seul, blancs, plusieurs lignes, code, retour arrière).
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
    let text = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).text().to_string());
    // Le texte chargé, puis `len` caractères sélectionnés à partir de `from` (en caractères).
    let pick = |cx: &mut gpui::VisualTestContext, note: &str, from: usize, len: usize| {
        let at = note.char_indices().nth(from).map_or(note.len(), |(i, _)| i);
        shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| e.load(note.to_string(), at, cx)));
        cx.run_until_parked();
        if len > 0 {
            cx.simulate_keystrokes(&"shift-right ".repeat(len));
        }
    };
    let selected = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).selected_text());
    let styles = |cx: &mut gpui::VisualTestContext| shell.update(cx, |s, cx| s.editor.update(cx, |e, _| e.styles()));
    // Chaque style : ses signes autour du mot, la sélection reste sur le mot, le même geste les retire.
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
        // Un seul retour arrière défait la pose ; refaire la remet.
        cx.simulate_keystrokes(key);
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(text(cx), "un mot ici", "{key} annulé");
        cx.simulate_keystrokes("secondary-shift-z");
        assert_eq!(text(cx), shown, "{key} rétabli");
    }
    // Plusieurs styles sur un mot, puis on en retire un du milieu : les autres restent.
    pick(cx, "un mot ici", 3, 3);
    cx.simulate_keystrokes("secondary-b secondary-u secondary-shift-h");
    assert_eq!(text(cx), "un **<u>==mot==</u>** ici");
    assert_eq!(styles(cx), markdown::BOLD | markdown::UNDER | markdown::HIGHLIGHT);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "un <u>==mot==</u> ici");
    cx.simulate_keystrokes("secondary-u");
    assert_eq!((text(cx), selected(cx).as_str()), ("un ==mot== ici".to_string(), "mot"));
    // Un gras autour d'un italique : le curseur dans l'italique retire le gras sans toucher l'italique.
    pick(cx, "**a *b* c**", 5, 0);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "a *b* c");
    // Le curseur seul : deux signes et le curseur entre eux ; la frappe les remplit ; le geste suivant les retire.
    pick(cx, "ab", 1, 0);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "a****b");
    cx.simulate_input("x");
    assert_eq!(text(cx), "a**x**b");
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "axb");
    // Paire vide : le geste la retire au lieu d'en empiler une autre.
    pick(cx, "ab", 1, 0);
    cx.simulate_keystrokes("secondary-b secondary-b");
    assert_eq!(text(cx), "ab");
    // Les blancs du bord restent dehors, la puce et le titre aussi.
    pick(cx, "un mot ici", 3, 4);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "un **mot** ici");
    pick(cx, "- liste", 0, 7);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "- **liste**");
    pick(cx, "# Titre", 2, 5);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "# **Titre**");
    // Que des blancs : rien ne bouge, et rien à annuler.
    pick(cx, "a   b", 1, 3);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "a   b");
    // Texte d'autres écritures, accents, émojis, caractères larges : les signes tombent aux bons octets.
    for note in ["日本語 é🙂", "é", "🙂", "a|b\\c \"d\""] {
        pick(cx, note, 0, note.chars().count());
        cx.simulate_keystrokes("secondary-b secondary-shift-h");
        assert_eq!(text(cx), format!("**=={note}==**"), "{note}");
        cx.simulate_keystrokes("secondary-shift-h secondary-b");
        assert_eq!(text(cx), note, "{note}");
    }
    // Plusieurs lignes : chacune a ses signes, sauf blocs de code et lignes vides ; un seul geste, un seul retour.
    let note = "- un\n- deux\n\n```\ncode\n```\nfin";
    pick(cx, note, 0, note.chars().count());
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "- **un**\n- **deux**\n\n```\ncode\n```\n**fin**");
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), note);
    cx.simulate_keystrokes("secondary-b secondary-z");
    assert_eq!(text(cx), note);
    // Un bloc de code non fermé n'en est pas un : son texte reçoit le style.
    pick(cx, "```\nsuite", 4, 5);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "```\n**suite**");
    // Dans un bloc de code, rien ne change.
    pick(cx, "```\ncode\n```", 5, 4);
    cx.simulate_keystrokes("secondary-b secondary-u");
    assert_eq!(text(cx), "```\ncode\n```");
    pick(cx, "```\ncode\n```", 6, 0);
    cx.simulate_keystrokes("secondary-b");
    assert_eq!(text(cx), "```\ncode\n```");
    // Une cellule de tableau, et un commentaire : la sélection y reste valide.
    pick(cx, "| a | b |\n| --- | --- |\n| c | d |", 26, 1);
    cx.simulate_keystrokes("secondary-b");
    assert!(text(cx).contains("| **c** |"), "{}", text(cx));
    // Plusieurs curseurs : chacun reçoit ses signes, un seul retour les défait tous.
    pick(cx, "un chat\nun chien", 0, 0);
    cx.simulate_keystrokes("secondary-d secondary-d secondary-u");
    assert_eq!(text(cx), "<u>un</u> chat\n<u>un</u> chien");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), "un chat\nun chien");
    cx.simulate_keystrokes("escape");

    // La barre du bas : au milieu de la page, à gauche du compteur, onze boutons de même taille.
    pick(cx, "un mot ici", 3, 3);
    let (bar, counts, pane) = (cx.debug_bounds("format-bar").unwrap(), cx.debug_bounds("counts").unwrap(), cx.debug_bounds("pane-0").unwrap());
    assert!(bar.right() <= counts.left() && bar.left() >= pane.left() && bar.bottom() <= pane.bottom(), "{bar:?} {counts:?} {pane:?}");
    // Au milieu de la page, ou un peu à gauche quand le compteur (long, avec une sélection) la pousse.
    assert!(bar.center().x <= pane.center().x + px(1.) && bar.center().x >= pane.center().x - px(60.), "{bar:?} {pane:?}");
    assert_eq!((bar.size.height, bar.size.width), (counts.size.height, px(335.)));
    const NAMES: [&str; 11] = ["format-0", "format-1", "format-2", "format-3", "format-4", "format-5", "format-6", "format-7", "format-8", "format-9", "format-10"];
    let buttons = |cx: &mut gpui::VisualTestContext| NAMES.iter().map(|name| cx.debug_bounds(name).unwrap_or_else(|| panic!("{name} absent"))).collect::<Vec<_>>();
    let all = buttons(cx);
    assert!(all.iter().all(|b| b.size == all[0].size && b.top() == all[0].top() && bar.contains(&b.center())), "{all:?}");
    assert!(all.windows(2).all(|w| w[0].right() <= w[1].left()), "{all:?}");
    // Un clic : le style se pose sur la sélection, qui reste, et le bouton s'allume ; un second clic le retire.
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
    // Les couleurs : le chevron ouvre les pastilles, une pastille surligne dans sa couleur (`<mark>`),
    // la même une seconde fois retire, une autre change la couleur ; H retire quelle qu'elle soit ; Échap ferme.
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
    // Sur plusieurs lignes, et le geste s'annule d'un coup ; Échap ferme les pastilles sans rien changer.
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
    assert!(shell.read_with(cx, |s, cx| !s.editor.read(cx).swatches_open()));
    assert_eq!(text(cx), "- a\n- b");
    // Le lien depuis la barre : la sélection devient le texte, le curseur attend l'adresse.
    pick(cx, "un mot ici", 3, 3);
    click(cx, 6);
    assert_eq!(text(cx), "un [mot]() ici");
    // Les listes : un clic change le genre de la ligne, le même bouton la remet en texte, un autre change de genre.
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
    // Le tableau : le bouton ouvre la grille de taille ; Entrée la pose. Dans le tableau, les listes cèdent la place à ses lignes et colonnes.
    pick(cx, "avant", 5, 0);
    click(cx, 10);
    cx.simulate_keystrokes("enter");
    assert!(text(cx).contains("|  |  |  |\n| --- | --- | --- |") || text(cx).contains("| --- |"), "{}", text(cx));
    assert!(shell.read_with(cx, |s, cx| s.editor.read(cx).in_table()));
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
    // Un seul geste d'annulation pour chacun.
    cx.simulate_keystrokes("secondary-z");
    assert_eq!((rows(cx), columns(cx)), (r, c + 1));
    // La barre, à toute largeur, ne disparaît jamais : au milieu de la page tant qu'il y a de la place, puis plus à
    // gauche, puis repliée en un bouton en bas à gauche, qui l'ouvre au-dessus de lui (ses boutons passent à la ligne).
    let mut seen = Vec::new();
    for width in [1400., 900., 700., 600., 560., 480., 420., 320., 240.] {
        cx.simulate_resize(size(px(width), px(600.)));
        cx.run_until_parked();
        for _ in 0..2 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
        // Les bornes d'un élément disparu restent en mémoire dans le test : c'est l'éditeur qui dit ce qui est à l'écran.
        let (shown, folded) = shell.read_with(cx, |s, cx| (s.editor.read(cx).bar_shown(), s.editor.read(cx).bar_folded()));
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
            // Repliée : le bouton est dans la note, au bord gauche, et le compteur ne passe pas dessous.
            let toggle = cx.debug_bounds("format-toggle").unwrap();
            assert!(toggle.left() >= pane.left() && toggle.right() <= counts.left().max(toggle.right()) && toggle.bottom() <= pane.bottom(), "{width} : {toggle:?} {counts:?} {pane:?}");
            assert!(counts.left() >= toggle.right() || counts.size.width == px(0.), "{width} : le compteur passe sous le bouton {toggle:?} {counts:?}");
            // Ouverte : tous les boutons sont dans la note, à la ligne si elle est étroite, au-dessus du bouton ; un clic agit.
            pick(cx, "un mot ici", 3, 3);
            let at = cx.debug_bounds("format-toggle").unwrap().center();
            cx.simulate_click(at, gpui::Modifiers::none());
            cx.run_until_parked();
            for _ in 0..2 {
                cx.update(|window, _| window.refresh());
                cx.run_until_parked();
            }
            assert!(shell.read_with(cx, |s, cx| s.editor.read(cx).bar_shown()), "{width} : le bouton ouvre la barre");
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
            // Les pastilles de couleur s'ouvrent aussi, au-dessus de la barre et dans la note.
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
            // Échap la referme sans toucher au texte.
            cx.simulate_keystrokes("escape");
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
            assert!(shell.read_with(cx, |s, cx| !s.editor.read(cx).bar_shown() && s.editor.read(cx).bar_folded()), "{width}");
            assert_eq!(text(cx), "un mot ici");
        }
        seen.push(shown);
    }
    assert!(seen[0] && !seen[8] && seen.windows(2).all(|w| w[0] >= w[1]), "elle se replie d'un seul bord, sans revenir en arrière : {seen:?}");
    // Deux panes : une seule barre, celle du pane où l'on écrit.
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
    // Réglage « Barre de mise en forme… » : masquée, elle ne se dessine plus et le choix survit au redémarrage ;
    // une valeur vide, absente ou inconnue la laisse affichée.
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
    assert!(!shell.read_with(cx, |s, cx| s.editor.read(cx).bar_shown()) && cx.debug_bounds("counts").is_some());
    assert!(vault::load_settings().contains("bar=off\n"));
    // Échap pendant la liste : le choix en aperçu est repris.
    choose(cx, "up escape");
    assert!(shell.read_with(cx, |s, _| s.prefs.bar == "off"));
    choose(cx, "up enter");
    assert!(shell.read_with(cx, |s, cx| s.theme.bar && s.editor.read(cx).bar_shown()));
    for (text, shown) in [("", true), ("bar=\n", true), ("bar=on\n", true), ("bar=zzz\n", true), ("bar=off\n", false)] {
        assert_eq!(Prefs::parse(text).bar != "off", shown, "{text:?}");
    }
    assert_eq!(Prefs::parse(&Prefs::parse("bar=off").to_text()).bar, "off");
    assert!(!Prefs::parse("").to_text().contains("bar="));
    let _ = fs::remove_dir_all(&root);
}
