//! Table rows and columns from the menu, the palette, the keyboard and the handles.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Un tableau : ses lignes et ses colonnes se gèrent depuis le menu, la palette ou le clavier,
    // et le curseur suit le geste.
    let flat = |cx: &mut gpui::VisualTestContext| {
        markdown::table(&text(cx).lines().filter(|l| l.starts_with('|')).collect::<Vec<_>>().join("\n")).0.iter().map(|r| r.iter().map(|c| if c.starts_with("--") { "---" } else { c }).collect::<Vec<_>>().join("|")).collect::<Vec<_>>().join("/")
    };
    let op = |cx: &mut gpui::VisualTestContext, op: editor::TableOp| {
        shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Table(op), None, window, cx));
    };
    use editor::TableOp::*;
    let grid = "| Nom | Âge |\n| --- | --- |\n| Léa | 31  |\n| Max | 7   |\n\nsuite";
    j.load(cx, grid, grid.find("Léa").unwrap());
    op(cx, RowBelow);
    assert_eq!(flat(cx), "Nom|Âge/---|---/Léa|31/|/Max|7");
    cx.simulate_input("Zoé");
    assert_eq!(flat(cx), "Nom|Âge/---|---/Léa|31/Zoé|/Max|7");
    cx.simulate_keystrokes("alt-up");
    assert_eq!(flat(cx), "Nom|Âge/---|---/Zoé|/Léa|31/Max|7");
    cx.simulate_keystrokes("alt-up");
    assert_eq!(flat(cx), "Nom|Âge/---|---/Zoé|/Léa|31/Max|7", "la première ligne ne monte pas plus haut");
    op(cx, ColRight);
    cx.simulate_input("Ville");
    assert_eq!(flat(cx), "Nom||Âge/---|---|---/Zoé|Ville|/Léa||31/Max||7");
    op(cx, MoveColLeft);
    assert_eq!(flat(cx), "|Nom|Âge/---|---|---/Ville|Zoé|/|Léa|31/|Max|7");
    op(cx, DeleteCol);
    assert_eq!(flat(cx), "Nom|Âge/---|---/Zoé|/Léa|31/Max|7");
    op(cx, SortDesc);
    assert_eq!(flat(cx), "Nom|Âge/---|---/Zoé|/Max|7/Léa|31");
    // Deux lignes choisies partent ensemble ; ni l'en-tête ni le corps ne laissent un tableau sans lignes.
    j.load(cx, grid, grid.find("Léa").unwrap());
    cx.simulate_keystrokes("shift-down");
    cx.simulate_keystrokes("secondary-shift-backspace");
    assert_eq!(flat(cx), "Nom|Âge/---|---");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(flat(cx), "Nom|Âge/---|---/Léa|31/Max|7");
    // Le tableau entier part avec son retour à la ligne ; annuler le rend.
    op(cx, DeleteTable);
    assert_eq!(text(cx), "\nsuite");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(flat(cx), "Nom|Âge/---|---/Léa|31/Max|7");
    // Un tableau indenté garde son retrait ; un tableau seul dans la note, sans retour à la ligne final, disparaît tout entier.
    let indented = "  | a | b |\n  | - | - |\n  | 1 | 2 |";
    j.load(cx, indented, indented.find('1').unwrap());
    op(cx, RowBelow);
    assert!(text(cx).lines().all(|l| l.starts_with("  | ")) && text(cx).lines().count() == 4, "{:?}", text(cx));
    cx.simulate_keystrokes("secondary-z");
    j.load(cx, "| a |\n| - |\n| b |", 8);
    op(cx, DeleteTable);
    assert_eq!(text(cx), "");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), "| a |\n| - |\n| b |");
    // Les poignées : une colonne prise se copie, s'efface, se supprime ; une ligne se tire plus haut ;
    // le « + » d'un trait insère ; l'angle prend tout le tableau.
    j.load(cx, grid, grid.find("Léa").unwrap());
    cx.run_until_parked();
    let center = |cx: &mut gpui::VisualTestContext, part: editor::Part| shell.read_with(cx, |s, cx| s.editor.read(cx).part_center(part).unwrap());
    let none = gpui::Modifiers::none();
    let at = center(cx, editor::Part::Col(1));
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-c");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("Âge\n31\n7"));
    cx.simulate_keystrokes("delete");
    assert_eq!(flat(cx), "Nom|/---|---/Léa|/Max|");
    cx.simulate_keystrokes("secondary-z");
    let at = center(cx, editor::Part::Col(0));
    cx.simulate_click(at, none);
    op(cx, DeleteCol);
    assert_eq!(flat(cx), "Âge/---/31/7");
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    let (from, to) = (center(cx, editor::Part::Row(2)), center(cx, editor::Part::JoinRow(1)));
    cx.simulate_mouse_down(from, MouseButton::Left, none);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    cx.simulate_mouse_up(to, MouseButton::Left, none);
    assert_eq!(flat(cx), "Nom|Âge/---|---/Max|7/Léa|31");
    // Le tableau déplacé reste pris : « monter » le prend encore.
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    let at = center(cx, editor::Part::JoinCol(1));
    cx.simulate_click(at, none);
    assert_eq!(flat(cx), "Nom||Âge/---|---|---/Léa||31/Max||7");
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    let at = center(cx, editor::Part::Corner);
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-c");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("Nom\tÂge\nLéa\t31\nMax\t7"));
    // Les limites des poignées. Un tableau d'une colonne et d'une ligne : supprimer sa colonne le supprime.
    use editor::Part;
    let one = "avant\n\n| a |\n| - |\n| b |\n\nsuite";
    j.load(cx, one, one.find('b').unwrap());
    cx.run_until_parked();
    let at = center(cx, Part::Col(0));
    cx.simulate_click(at, none);
    op(cx, DeleteCol);
    assert!(!text(cx).contains('|') && text(cx).contains("suite") && text(cx).starts_with("avant"), "{:?}", text(cx));
    // L'en-tête se prend mais ne se tire pas ; une ligne lâchée où elle est ne bouge rien, ni sous la
    // dernière, ni sur son propre trait ; une colonne tirée au-delà de la dernière passe en dernier.
    j.load(cx, grid, grid.find("Léa").unwrap());
    cx.run_until_parked();
    let before = flat(cx);
    let (head, below) = (center(cx, Part::Row(0)), center(cx, Part::JoinRow(2)));
    cx.simulate_mouse_down(head, MouseButton::Left, none);
    cx.simulate_mouse_move(below, MouseButton::Left, none);
    cx.simulate_mouse_up(below, MouseButton::Left, none);
    assert_eq!(flat(cx), before);
    for (row, to) in [(2, 3), (2, 2), (1, 1), (1, 2)] {
        let (from, to) = (center(cx, Part::Row(row)), center(cx, Part::JoinRow(to)));
        cx.simulate_mouse_down(from, MouseButton::Left, none);
        cx.simulate_mouse_move(to, MouseButton::Left, none);
        cx.simulate_mouse_up(to, MouseButton::Left, none);
        assert_eq!(flat(cx), before, "ligne {row} lâchée sur le trait {to}");
    }
    let (from, to) = (center(cx, Part::Col(0)), center(cx, Part::JoinCol(2)));
    cx.simulate_mouse_down(from, MouseButton::Left, none);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    cx.simulate_mouse_up(to, MouseButton::Left, none);
    assert_eq!(flat(cx), "Âge|Nom/---|---/31|Léa/7|Max");
    // Échap en plein glisser : rien ne bouge.
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    let (from, to) = (center(cx, Part::Row(1)), center(cx, Part::JoinRow(3)));
    cx.simulate_mouse_down(from, MouseButton::Left, none);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(to, MouseButton::Left, none);
    assert_eq!(flat(cx), before);
    // Le « + » du dernier trait ajoute une ligne à la fin ; celui du premier, une colonne en tête.
    let at = center(cx, Part::JoinRow(3));
    cx.simulate_click(at, none);
    assert_eq!(flat(cx), "Nom|Âge/---|---/Léa|31/Max|7/|");
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    let at = center(cx, Part::JoinCol(0));
    cx.simulate_click(at, none);
    assert_eq!(flat(cx), "|Nom|Âge/---|---|---/|Léa|31/|Max|7");
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    // L'angle puis Effacer vide toutes les cases sans changer la forme ; Échap relâche la prise.
    let at = center(cx, Part::Corner);
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("delete");
    assert_eq!(flat(cx), "|/---|---/|/|");
    cx.simulate_keystrokes("secondary-z");
    let at = center(cx, Part::Corner);
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("secondary-shift-backspace");
    // Sans prise, le geste vaut pour la ligne du curseur : l'en-tête, et la suivante lui succède.
    assert_eq!(flat(cx), "Léa|31/---|---/Max|7");
    cx.simulate_keystrokes("secondary-z");
    // Une colonne prise par sa poignée : le même geste la supprime, elle et non la ligne.
    let at = center(cx, Part::Col(0));
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-shift-backspace");
    assert_eq!(flat(cx), "Âge/---/31/7");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(flat(cx), "Nom|Âge/---|---/Léa|31/Max|7");
    // Trente colonnes : celles qui sortent du cadre n'ont pas de poignée à l'écran ; un tableau
    // de plus de 1 500 lignes n'en a aucune (il garde Tab).
    let wide = format!("|{}\n|{}\n|{}\n", " colonne |".repeat(30), " --- |".repeat(30), " texte |".repeat(30));
    j.load(cx, &wide, 0);
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, cx| s.editor.read(cx).part_center(Part::Col(0)).is_some() && s.editor.read(cx).part_center(Part::Col(29)).is_none()));
    let big = format!("| a | b |\n| - | - |\n{}", "| 1 | 2 |\n".repeat(grid::BIG + 5));
    j.load(cx, &big, 0);
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, cx| s.editor.read(cx).part_center(Part::Col(0)).is_none() && !s.editor.read(cx).in_table()));
    // Dans le menu de la note : les gestes du tableau, et sa palette.
    j.load(cx, grid, grid.find("Léa").unwrap());
    assert!(shell.read_with(cx, |s, cx| s.editor.read(cx).in_table()));
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("colonne à droite");
    cx.simulate_keystrokes("down enter");
    assert_eq!(flat(cx), "Nom||Âge/---|---|---/Léa||31/Max||7");
    // Tout le tableau pris (l'angle) : la colonne vient à droite de la dernière.
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    let at = center(cx, Part::Corner);
    cx.simulate_click(at, none);
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("colonne à droite");
    cx.simulate_keystrokes("down enter");
    assert_eq!(flat(cx), "Nom|Âge|/---|---|---/Léa|31|/Max|7|");
    // La note ouverte ici n'existe pas sur le disque : l'enregistrement échoue, et son avis s'efface.
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
}
