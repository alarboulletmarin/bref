//! The diagram canvas, `![[` completion and `/` commands.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Schéma : Ctrl+Maj+D en crée un à côté de la note. Une lettre choisit la
    // forme, glisser la pose ; Entrée écrit dedans ; une flèche tirée d'une
    // forme à l'autre s'y accroche.
    cx.simulate_keystrokes("secondary-shift-d");
    cx.run_until_parked();
    let (file, canvas) = shell.read_with(cx, |s, _| s.drawing.clone().unwrap());
    assert_eq!(file.file_name().unwrap(), "Schéma.svg");
    let drag = |cx: &mut gpui::VisualTestContext, from: (f32, f32), to: (f32, f32)| {
        let (from, to) = canvas.read_with(cx, |c, _| (c.spot(from.0, from.1), c.spot(to.0, to.1)));
        cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    };
    let saved = || Diagram::from_svg(&fs::read_to_string(&file).unwrap()).unwrap();
    cx.simulate_input("r");
    drag(cx, (0., 0.), (120., 60.));
    cx.simulate_keystrokes("enter");
    cx.simulate_input("Client");
    cx.simulate_keystrokes("escape");
    cx.simulate_input("o");
    drag(cx, (300., 0.), (400., 80.));
    cx.simulate_input("a");
    drag(cx, (60., 30.), (350., 40.));
    let drawn = saved();
    assert_eq!((drawn.shapes.len(), drawn.shapes[0].text.as_str(), drawn.shapes[0].w), (2, "Client", 120.));
    let ends = (diagram::End::Shape(drawn.shapes[0].id), diagram::End::Shape(drawn.shapes[1].id));
    assert_eq!((drawn.links[0].from, drawn.links[0].to, drawn.links[0].end), (ends.0, ends.1, diagram::Head::Arrow));
    // Tirée du bord d'une forme, la flèche s'y fixe ; elle est coudée d'office.
    cx.simulate_input("a");
    drag(cx, (115., 30.), (350., 40.));
    let pinned = saved().links[1].clone();
    assert_eq!((pinned.from, pinned.route), (diagram::End::Pin(drawn.shapes[0].id, 1., 0.5), diagram::Route::Elbow));
    // « + » et « - » zooment, « 0 » recadre tout.
    let span = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| f32::from(c.spot(100., 0.).x - c.spot(0., 0.).x));
    cx.simulate_input("+");
    assert_eq!(span(cx), 125.);
    cx.simulate_input("-");
    cx.simulate_input("0");
    cx.run_until_parked();
    assert!(span(cx) <= 100.);
    // Une forme se déplace en la tirant, sur la grille ; Ctrl+Z la remet en place.
    drag(cx, (350., 40.), (350., 143.));
    assert_eq!(saved().shapes[1].y, 100.);
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(saved().shapes[1].y, 0.);
    // Ctrl+clic ajoute une forme à la sélection ; un second l'en retire.
    let click = |cx: &mut gpui::VisualTestContext, at: (f32, f32), held: gpui::Modifiers| {
        let at = canvas.read_with(cx, |c, _| c.spot(at.0, at.1));
        cx.simulate_click(at, held);
    };
    click(cx, (20., 50.), gpui::Modifiers::none());
    click(cx, (350., 40.), gpui::Modifiers::secondary_key());
    assert_eq!(canvas.read_with(cx, |c, _| c.picked()), 2);
    click(cx, (350., 40.), gpui::Modifiers::secondary_key());
    assert_eq!(canvas.read_with(cx, |c, _| c.picked()), 1);
    // Suppr retire la forme sélectionnée, et la flèche qui y tenait.
    drag(cx, (350., 40.), (350., 40.));
    cx.simulate_keystrokes("delete");
    assert_eq!((saved().shapes.len(), saved().links.len()), (1, 0));
    // Le schéma figure parmi les images du coffre ; Échap rend la main à la note.
    assert!(shell.read_with(cx, |s, _| s.images.contains(&file)));
    cx.simulate_keystrokes("escape escape");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.picture.is_none() && s.drawing.is_none()));

    // `![[` propose les images et les schémas du coffre ; Entrée complète le nom.
    cx.simulate_keystrokes("secondary-end enter");
    cx.simulate_input("![[sch");
    cx.simulate_keystrokes("enter");
    assert!(text(cx).ends_with("![[Schéma.svg]]"));
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();

    // Commandes `/` : la liste se filtre par nom ou par libellé ; Entrée pose le
    // composant. Ici un panneau, dont la citation continue à la ligne.
    cx.simulate_keystrokes("enter");
    cx.simulate_input("/pan");
    cx.simulate_keystrokes("down enter");
    cx.simulate_input("vu");
    assert!(text(cx).ends_with("\n> [!TIP]\n> vu"));
    cx.simulate_keystrokes("enter enter");
    // `/tableau` ouvre la grille : les flèches règlent colonnes et lignes.
    cx.simulate_input("/tableau");
    cx.simulate_keystrokes("enter left down enter");
    cx.simulate_input("Nom");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("Âge");
    // Tab passe à la cellule suivante, par-dessus les tirets, et réaligne les colonnes.
    cx.simulate_keystrokes("tab");
    cx.simulate_input("Élodie");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("31");
    cx.simulate_keystrokes("tab");
    let empty = "|        |     |";
    let table = format!("| Nom    | Âge |\n| ------ | --- |\n| Élodie | 31  |\n{empty}\n{empty}");
    assert!(text(cx).ends_with(&format!("\n\n{table}")), "{}", text(cx));
    // Entrée ajoute une ligne sous celle du curseur ; les boutons « + » une
    // colonne à droite et une ligne en bas.
    cx.simulate_keystrokes("enter");
    assert!(text(cx).ends_with(&format!("{table}\n{empty}")));
    cx.run_until_parked();
    let [_, column] = shell.read_with(cx, |s, cx| s.editor.read(cx).plus_buttons().unwrap());
    cx.simulate_click(column, gpui::Modifiers::none());
    cx.simulate_input("Ville");
    // Chaque frappe réaligne les colonnes ; effacer aussi.
    assert!(text(cx).contains("| Nom    | Âge | Ville |\n| ------ | --- | ----- |\n| Élodie | 31  |       |"), "{}", text(cx));
    cx.simulate_input("s x");
    cx.simulate_keystrokes("backspace backspace backspace");
    assert!(text(cx).contains("| Nom    | Âge | Ville |\n| ------ | --- | ----- |\n| Élodie | 31  |       |"), "{}", text(cx));
    cx.run_until_parked();
    let [row, _] = shell.read_with(cx, |s, cx| s.editor.read(cx).plus_buttons().unwrap());
    cx.simulate_click(row, gpui::Modifiers::none());
    // Entrée sur une dernière ligne vide la retire et sort du tableau.
    cx.simulate_keystrokes("enter");
    cx.simulate_input("fin");
    let empty = "|        |     |       |";
    let table = format!("| Nom    | Âge | Ville |\n| ------ | --- | ----- |\n| Élodie | 31  |       |\n{empty}\n{empty}\n{empty}");
    assert!(text(cx).ends_with(&format!("\n\n{table}\nfin")), "{}", text(cx));
    // À la souris : la case survolée de la grille donne la taille du tableau.
    cx.simulate_keystrokes("enter");
    cx.simulate_input("/table");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let cell = shell.read_with(cx, |s, cx| s.editor.read(cx).grid_cell(2, 1).unwrap());
    cx.simulate_click(cell, gpui::Modifiers::none());
    assert!(text(cx).ends_with("fin\n\n|     |     |\n| --- | --- |"), "{}", text(cx));
    // Au clavier : une taille tapée dans la grille, au-delà de ses cases.
    cx.simulate_keystrokes("secondary-end enter enter");
    cx.simulate_input("/table");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("12x9");
    cx.simulate_keystrokes("backspace");
    cx.simulate_input("1");
    cx.simulate_keystrokes("enter");
    let wide = format!("|{}\n|{}", "     |".repeat(12), " --- |".repeat(12));
    assert!(text(cx).ends_with(&format!("| --- | --- |\n\n{wide}")), "{}", text(cx));
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
}
