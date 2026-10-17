//! Code blocks, images, pasted images, Mermaid, foreign SVG and the viewer.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let (shell, root) = (&j.shell, &j.root);
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Bloc de code : Entrée pose la clôture ; curseur dedans, la copie rapide ne
    // prend que le bloc, et le bouton « Copier » de son ouverture fait de même.
    cx.simulate_keystrokes("secondary-end enter");
    cx.simulate_input("```rust");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("let x = 1;");
    assert!(text(cx).ends_with("```rust\nlet x = 1;\n```"));
    let clipboard = |cx: &mut gpui::VisualTestContext| cx.read_from_clipboard().and_then(|item| item.text());
    cx.simulate_keystrokes("secondary-shift-c");
    assert_eq!(clipboard(cx).as_deref(), Some("let x = 1;"));
    cx.simulate_keystrokes("secondary-home secondary-shift-c");
    assert!(clipboard(cx).unwrap().starts_with("# Titre"));
    let button = shell.read_with(cx, |s, cx| s.editor.read(cx).copy_button().unwrap());
    cx.simulate_click(button, gpui::Modifiers::none());
    assert_eq!(clipboard(cx).as_deref(), Some("let x = 1;"));

    // Image : la ligne qui la désigne porte l'image, trouvée à côté de la note.
    // Les signes (`->`) ne s'affichent que hors de la ligne du curseur, sans toucher au texte.
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20"/></svg>"#;
    fs::write(root.join("carré.svg"), svg).unwrap();
    cx.simulate_keystrokes("secondary-end enter");
    cx.simulate_input("a -> b ![](carré.svg)");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_input("fin");
    cx.run_until_parked();
    let (signs, image) = shell.read_with(cx, |s, cx| s.editor.read(cx).decorations("a -> b"));
    assert_eq!((signs, image), (1, Some((40., 20.))));
    assert!(text(cx).ends_with("a -> b ![](carré.svg)\nfin"));

    // Coller une image l'enregistre à côté de la note, qui la désigne.
    let image = gpui::Image::from_bytes(gpui::ImageFormat::Svg, svg.as_bytes().to_vec());
    cx.write_to_clipboard(ClipboardItem::new_image(&image));
    cx.simulate_keystrokes("enter secondary-v");
    let note = text(cx);
    let pasted = note.rsplit_once("![](").unwrap().1.trim_end_matches(')');
    assert!(pasted.starts_with("image-") && pasted.ends_with(".svg"));
    assert_eq!(fs::read_to_string(root.join(pasted)).unwrap(), svg);

    // Diagramme Mermaid, dessiné sous son bloc une fois le curseur sorti, et
    // formule LaTeX, sous sa ligne.
    cx.simulate_keystrokes("enter");
    cx.simulate_input("```mermaid");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("flowchart LR");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("A --> B");
    cx.simulate_keystrokes("secondary-end enter");
    cx.simulate_input("soit $x^2$");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_input("fin");
    cx.run_until_parked();
    let figures = shell.read_with(cx, |s, cx| {
        let editor = s.editor.read(cx);
        ["![](image-", "```\nsoit", "soit $x^2$"].map(|line| editor.decorations(line).1.is_some())
    });
    assert_eq!(figures, [true, true, true]);

    // Les images du coffre sont suivies comme les notes. Celles qu'une note affiche
    // ont leur nœud dans le graphe ; en choisir une l'affiche à la place de la note.
    for _ in 0..2 {
        cx.executor().advance_clock(Duration::from_secs(3));
        cx.run_until_parked();
    }
    cx.simulate_keystrokes("secondary-g");
    cx.run_until_parked();
    let (images, notes, nodes) = shell.read_with(cx, |s, cx| {
        (s.images.len(), s.notes.len(), s.graph.read(cx).size().0)
    });
    assert_eq!((images, nodes), (2, notes + 2));
    // Une image qu'aucune note n'affiche a aussi son nœud, sans lien.
    fs::write(root.join("seule.svg"), svg).unwrap();
    for _ in 0..2 {
        cx.executor().advance_clock(Duration::from_secs(3));
        cx.run_until_parked();
    }
    assert_eq!(shell.read_with(cx, |s, cx| s.graph.read(cx).size().0), notes + 3);
    shell.update(cx, |s, cx| s.preview_note(&root.join("carré.svg"), cx));
    assert_eq!(shell.read_with(cx, |s, _| s.picture.clone()), Some(root.join("carré.svg")));
    assert!(text(cx).ends_with("fin"));
    // Revenir d'une image à la note qu'elle cachait se fait en une sélection, comme
    // d'une note à une autre.
    shell.update(cx, |s, cx| {
        let note = s.path.clone().unwrap();
        s.preview_note(&note, cx);
        assert_eq!(s.picture, None);
        s.preview_note(&root.join("carré.svg"), cx);
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| s.picture.clone()), None);

    // Un SVG d'ailleurs (draw.io : texte en HTML, couleurs `light-dark()`) s'affiche dans la
    // note, dessiné en tâche de fond. Changer de thème, même vite et plusieurs fois, garde la
    // figure à sa place (l'ancienne reste le temps que la nouvelle se dessine) et n'accumule rien.
    let drawio = r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="120" style="background: light-dark(#fff, #121212)"><rect width="400" height="120" style="fill: light-dark(#ffffff, #121212)"/><switch><foreignObject width="100%" height="100%" requiredFeatures="http://www.w3.org/TR/SVG11/feature#Extensibility"><div xmlns="http://www.w3.org/1999/xhtml" style="display: flex; align-items: unsafe center; justify-content: unsafe center; width: 200px; height: 1px; padding-top: 60px; margin-left: 100px;"><div style="text-align: center;"><div style="display: inline-block; font-size: 12px; color: light-dark(#000, #fff); white-space: normal;"><b>Étape</b><br/>texte</div></div></div></foreignObject><text x="200" y="64">Text is not SVG - cannot display</text></switch></svg>"##;
    fs::write(root.join("drawio.svg"), drawio).unwrap();
    fs::write(root.join("cassé.svg"), "<svg pas fermé").unwrap();
    cx.simulate_keystrokes("secondary-end enter");
    cx.simulate_input("![](drawio.svg)");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("![](cassé.svg)");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_input("fin");
    cx.run_until_parked();
    let seen = |cx: &mut gpui::VisualTestContext| {
        shell.read_with(cx, |s, cx| {
            let editor = s.editor.read(cx);
            (editor.decorations("![](drawio").1, editor.decorations("![](cassé").1, editor.figure_count())
        })
    };
    let (drawn, broken, (count, waiting)) = seen(cx);
    assert_eq!((drawn, broken, waiting), (Some((400., 120.)), None, 0));
    for name in ["Bref Dark", "Bref Light", "Bref Dark", "Bref Light", "Bref Dark"] {
        shell.update_in(cx, |s, window, cx| {
            s.prefs.theme = name.into();
            s.restyle(window, cx);
        });
        cx.simulate_input("x");
        cx.simulate_keystrokes("backspace");
        assert_eq!(seen(cx).0, Some((400., 120.)), "la figure reste en place pendant qu'elle se redessine");
    }
    cx.run_until_parked();
    cx.simulate_input("x");
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    assert_eq!(seen(cx), (Some((400., 120.)), None, (count, 0)), "ni figure ni dessin de trop");

    // Le même SVG, ouvert, s'affiche dans la visionneuse : cadré sur sa largeur, zoomé au
    // clavier, et Échap ramène à la note. Un SVG illisible le dit, sans rien casser.
    // Ouvert depuis l'arbre, il prend la saisie : la note ne la garde pas.
    shell.update_in(cx, |s, window, cx| {
        s.preview_note(&root.join("drawio.svg"), cx);
        s.focus_pane(0, window, cx);
    });
    // La plateforme de test ne redessine pas d'elle-même quand un dessin en tâche de fond aboutit.
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..3 {
            cx.run_until_parked();
            cx.update(|window, _| window.refresh());
        }
        cx.run_until_parked();
    };
    settle(cx);
    let viewer = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.viewer.as_ref().map(|(_, v)| v.read(cx).state()));
    let (loaded, failed, fit, _) = viewer(cx).unwrap();
    assert!(loaded && !failed && fit > 0. && fit <= 1., "{:?}", viewer(cx));
    cx.simulate_keystrokes("secondary-=");
    assert!((viewer(cx).unwrap().2 - fit * 1.25).abs() < 1e-4);
    cx.simulate_keystrokes("secondary-0");
    settle(cx);
    assert_eq!(viewer(cx).unwrap().2, fit);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.picture.is_none() && s.viewer.is_none()));
    // La loupe d'un bloc Mermaid montre le diagramme en grand, à la place de la note ; + et -
    // zooment ; Échap ramène à la note.
    let button = shell.read_with(cx, |s, cx| s.editor.read(cx).view_button("```mermaid")).unwrap();
    cx.simulate_click(button, gpui::Modifiers::none());
    settle(cx);
    let figure = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.figure.as_ref().map(|f| f.read(cx).state()));
    let (loaded, failed, fit, _) = figure(cx).unwrap();
    assert!(loaded && !failed && fit > 0., "{:?}", figure(cx));
    assert!(text(cx).contains("```mermaid\nflowchart LR"), "la note n'a pas bougé");
    cx.simulate_keystrokes("+");
    assert!((figure(cx).unwrap().2 - fit * 1.25).abs() < 1e-4);
    cx.simulate_keystrokes("- 0");
    settle(cx);
    assert_eq!(figure(cx).unwrap().2, fit);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.figure.is_none()));
    shell.update_in(cx, |s, window, cx| {
        s.preview_note(&root.join("cassé.svg"), cx);
        s.focus_pane(0, window, cx);
    });
    settle(cx);
    assert_eq!(viewer(cx).map(|v| (v.0, v.1)), Some((false, true)));
    shell.update(cx, |s, cx| {
        let note = s.path.clone().unwrap();
        s.preview_note(&note, cx);
    });
    cx.run_until_parked();
}
