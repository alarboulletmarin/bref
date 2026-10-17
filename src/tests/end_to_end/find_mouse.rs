//! Find in the note, checkboxes, double and triple click, import, new note and folder.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let (shell, root) = (&j.shell, &j.root);
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Recherche dans la note : Ctrl+F, la frappe montre le premier passage, Entrée et
    // Maj+Entrée tournent en boucle, Alt+C tient compte de la casse, Alt+W des mots entiers.
    j.load(cx, "Été, été.\n\n| a | étés |\n| - | ---- |\n| b | ÉTÉ  |\n", 0);
    let finding = |cx: &mut gpui::VisualTestContext| {
        shell.read_with(cx, |s, cx| s.editor.read(cx).finding().map(|(query, at, n)| (query.to_string(), at, n)))
    };
    let span = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).span());
    cx.simulate_keystrokes("secondary-f");
    cx.simulate_input("étéx");
    assert_eq!(finding(cx), Some(("étéx".into(), 0, 0)));
    cx.simulate_keystrokes("backspace");
    assert_eq!((finding(cx), span(cx)), (Some(("été".into(), 1, 4)), 0.."Été".len()));
    cx.simulate_keystrokes("enter enter enter enter");
    assert_eq!(finding(cx), Some(("été".into(), 1, 4)));
    cx.simulate_keystrokes("shift-enter");
    assert_eq!(finding(cx), Some(("été".into(), 4, 4)));
    cx.simulate_keystrokes("alt-c");
    assert_eq!(finding(cx), Some(("été".into(), 1, 2)));
    cx.simulate_keystrokes("alt-w");
    assert_eq!((finding(cx), span(cx)), (Some(("été".into(), 1, 1)), "Été, ".len().."Été, été".len()));
    cx.simulate_keystrokes("alt-c alt-w");
    assert_eq!(finding(cx), Some(("été".into(), 2, 4)));
    // Remplacer : Ctrl+H ouvre le second champ, Tab y passe, Entrée remplace le passage
    // courant, Ctrl+Entrée tous les autres ; Échap ferme, et un seul Ctrl+Z les rend tous.
    cx.simulate_keystrokes("secondary-h tab");
    cx.simulate_input("hiver");
    cx.simulate_keystrokes("enter");
    assert!(text(cx).starts_with("Été, hiver.\n"));
    assert_eq!(finding(cx), Some(("été".into(), 2, 3)));
    cx.simulate_keystrokes("secondary-enter");
    assert!(text(cx).starts_with("hiver, hiver.\n") && text(cx).contains("hivers") && !text(cx).to_lowercase().contains("été"));
    assert_eq!(finding(cx), Some(("été".into(), 0, 0)));
    cx.simulate_keystrokes("escape");
    assert_eq!(finding(cx), None);
    cx.simulate_keystrokes("secondary-z");
    assert!(text(cx).starts_with("Été, hiver.\n") && text(cx).contains("ÉTÉ"));
    // La sélection (ici le passage que l'annulation a rendu) devient la recherche ; la
    // première frappe la remplace. Barre ouverte, saisie rendue à la note : les passages
    // suivent le texte, Échap ferme.
    cx.simulate_keystrokes("secondary-f");
    assert!(finding(cx).is_some_and(|(query, ..)| query.to_lowercase().starts_with("été")));
    cx.simulate_input("hiver");
    assert_eq!(finding(cx), Some(("hiver".into(), 1, 1)));
    shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| e.load("hiver hiver".into(), 0, cx)));
    cx.run_until_parked();
    assert_eq!(finding(cx), Some(("hiver".into(), 1, 2)));
    cx.simulate_input("x");
    assert_eq!((text(cx), finding(cx)), ("xhiver hiver".into(), Some(("hiver".into(), 1, 2))));
    // La saisie est dans la note : F3 et les réglages de la barre répondent quand même.
    cx.simulate_keystrokes("f3");
    assert_eq!(finding(cx), Some(("hiver".into(), 2, 2)));
    cx.simulate_keystrokes("alt-w");
    assert_eq!(finding(cx), Some(("hiver".into(), 1, 1)));
    cx.simulate_keystrokes("alt-w alt-c alt-c escape");
    assert_eq!(finding(cx), None);
    // Sans barre, F3 l'ouvre.
    cx.simulate_keystrokes("f3");
    assert!(finding(cx).is_some());
    cx.simulate_keystrokes("escape");
    assert_eq!(finding(cx), None);
    // Expression régulière : Alt+R ; le remplacement reprend ses groupes.
    j.load(cx, "le chat, le chien", 0);
    cx.simulate_keystrokes("secondary-h");
    cx.simulate_input(r"le (\w+)");
    assert_eq!(finding(cx), Some((r"le (\w+)".into(), 0, 0)));
    cx.simulate_keystrokes("alt-r");
    assert_eq!(finding(cx), Some((r"le (\w+)".into(), 1, 2)));
    cx.simulate_keystrokes("tab");
    cx.simulate_input("$1 !");
    cx.simulate_keystrokes("enter");
    assert_eq!(text(cx), "chat !, le chien");
    cx.simulate_keystrokes("secondary-enter");
    assert_eq!(text(cx), "chat !, chien !");
    cx.simulate_keystrokes("alt-r escape");
    assert_eq!(finding(cx), None);

    // Cases à cocher : dans le texte comme dans un tableau, un clic les coche, Ctrl+Entrée aussi.
    let boxed = "avant [ ] fait [x] fin\n\n| Tâche | Fait |\n| ----- | ---- |\n| a     | [ ]  |\n| b     |      |\n";
    j.load(cx, boxed, boxed.len());
    let tick = |cx: &mut gpui::VisualTestContext, needle: &str| {
        let at = boxed.find(needle).unwrap() + 1;
        let spot = shell.read_with(cx, |s, cx| s.editor.read(cx).point_of(at).unwrap()) + gpui::point(px(3.), px(0.));
        cx.simulate_click(spot, gpui::Modifiers::none());
    };
    tick(cx, "[ ] fait");
    assert!(text(cx).starts_with("avant [x] fait [x] fin"), "{}", text(cx));
    tick(cx, "[x] fin");
    assert!(text(cx).starts_with("avant [x] fait [ ] fin"), "{}", text(cx));
    tick(cx, "[ ]  |");
    assert!(text(cx).ends_with("| a     | [x]  |\n| b     |      |\n"), "{}", text(cx));
    cx.simulate_keystrokes("secondary-z");
    assert!(text(cx).ends_with("| a     | [ ]  |\n| b     |      |\n"), "{}", text(cx));
    // Le pointeur prend la main sur une case, et sur un lien quand Ctrl est tenu ; ailleurs, la barre du texte.
    let hand = |cx: &mut gpui::VisualTestContext, needle: &str, held: gpui::Modifiers| {
        let at = boxed.find(needle).unwrap() + 1;
        let spot = shell.read_with(cx, |s, cx| s.editor.read(cx).point_of(at).unwrap()) + gpui::point(px(3.), px(0.));
        cx.simulate_mouse_move(spot, None, held);
        shell.read_with(cx, |s, cx| s.editor.read(cx).hand())
    };
    assert_eq!(hand(cx, "[x] fin", gpui::Modifiers::none()), Some(CursorStyle::PointingHand));
    assert_eq!(hand(cx, "avant", gpui::Modifiers::none()), None);
    let linked = "voir [[Courses]] ici\n";
    j.load(cx, linked, 0);
    let over = |cx: &mut gpui::VisualTestContext, held: gpui::Modifiers| {
        let spot = shell.read_with(cx, |s, cx| s.editor.read(cx).point_of(linked.find("Courses").unwrap()).unwrap()) + gpui::point(px(3.), px(0.));
        cx.simulate_mouse_move(spot, None, held);
        shell.read_with(cx, |s, cx| s.editor.read(cx).hand())
    };
    assert_eq!(over(cx, gpui::Modifiers::none()), None);
    assert_eq!(over(cx, gpui::Modifiers::secondary_key()), Some(CursorStyle::PointingHand));
    // Une case dans le code reste du texte.
    j.load(cx, "`[ ]` et\n\n```\n[ ]\n```\n", 0);
    let code = shell.read_with(cx, |s, cx| s.editor.read(cx).point_of(2).unwrap()) + gpui::point(px(3.), px(0.));
    cx.simulate_click(code, gpui::Modifiers::none());
    assert!(text(cx).starts_with("`[ ]` et"));
    // Ctrl+Entrée dans une cellule : elle reçoit une case, puis la coche.
    j.load(cx, boxed, boxed.find("| b").unwrap() + "| b     | ".len());
    cx.simulate_keystrokes("secondary-enter");
    assert!(text(cx).ends_with("| b     | [ ]  |\n"), "{}", text(cx));
    cx.simulate_keystrokes("secondary-enter");
    assert!(text(cx).ends_with("| b     | [x]  |\n"), "{}", text(cx));

    // Double clic puis glisser : la sélection s'étend de mot en mot ; le triple, de ligne en ligne.
    let words = "alpha beta gamma delta\nsecond\ntroisième\n";
    j.load(cx, words, 0);
    let spot = |cx: &mut gpui::VisualTestContext, i: usize| {
        shell.read_with(cx, |s, cx| s.editor.read(cx).point_of(i).unwrap()) + gpui::point(px(3.), px(0.))
    };
    let selected = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).selected().to_string());
    let (beta, gamma) = (spot(cx, 7), spot(cx, 13));
    let click = |count| gpui::MouseDownEvent {
        position: beta,
        modifiers: gpui::Modifiers::none(),
        button: gpui::MouseButton::Left,
        click_count: count,
        first_mouse: false,
    };
    cx.simulate_event(click(2));
    assert_eq!(selected(cx), "beta");
    cx.simulate_mouse_move(gamma, gpui::MouseButton::Left, gpui::Modifiers::none());
    assert_eq!(selected(cx), "beta gamma");
    let alpha = spot(cx, 1);
    cx.simulate_mouse_move(alpha, gpui::MouseButton::Left, gpui::Modifiers::none());
    assert_eq!(selected(cx), "alpha beta");
    cx.simulate_mouse_up(alpha, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(gamma, None, gpui::Modifiers::none());
    assert_eq!(selected(cx), "alpha beta");
    cx.simulate_event(click(3));
    let third = spot(cx, words.find("troi").unwrap() + 2);
    cx.simulate_mouse_move(third, gpui::MouseButton::Left, gpui::Modifiers::none());
    assert_eq!(selected(cx), "alpha beta gamma delta\nsecond\ntroisième");
    cx.simulate_mouse_up(third, gpui::MouseButton::Left, gpui::Modifiers::none());

    // Import : un fichier Excalidraw devient un schéma du coffre, sous son nom.
    // Son image PNG s'enregistre à côté de lui.
    let outside = root.join(".config/croquis.excalidraw");
    fs::write(&outside, r#"{"elements":[{"id":"a","type":"ellipse","x":0,"y":0,"width":80,"height":60}]}"#).unwrap();
    shell.update_in(cx, |s, window, cx| s.import_file(&outside, window, cx));
    cx.run_until_parked();
    let (file, canvas) = shell.read_with(cx, |s, _| s.drawing.clone().unwrap());
    assert_eq!((file.file_name().unwrap().to_str(), canvas.read_with(cx, |c, _| c.diagram().shapes.len())), (Some("croquis.svg"), 1));
    shell.update(cx, |s, cx| s.export_png(file.clone(), &canvas, cx));
    cx.run_until_parked();
    assert!(fs::read(file.with_extension("png")).unwrap().starts_with(b"\x89PNG"));
    assert!(shell.read_with(cx, |s, _| s.images.contains(&file.with_extension("png")) && !s.told(Tone::Failed, "")));

    // Le bouton « nouvelle note » de l'arbre range à la racine du coffre, même avec un dossier sélectionné.
    shell.update_in(cx, |s, _, cx| {
        s.nav.panel = Panel::Split;
        s.nav.mode = Mode::Tree;
        s.nav.sel = Some(root.join("Projets"));
        s.flush(cx);
    });
    shell.update_in(cx, |s, window, cx| s.new_note_in(None, window, cx));
    cx.simulate_input("# Racine");
    shell.update(cx, |s, cx| s.flush(cx));
    assert!(root.join("Racine.md").is_file() && !root.join("Projets/Racine.md").exists());

    // Ctrl+P : « Nouveau dossier » ouvre le champ de saisie du nom, le dossier naît dans le coffre.
    shell.update(cx, |s, _| s.nav.sel = None);
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("nouveau dossier");
    cx.simulate_keystrokes("down enter");
    cx.simulate_input("Archives");
    cx.simulate_keystrokes("enter");
    assert!(root.join("Archives").is_dir(), "dossier créé depuis la palette");
}
