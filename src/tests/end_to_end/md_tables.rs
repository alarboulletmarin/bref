//! Markdown tables: wrapping, pasting, moving between cells, random edits, front matter.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Un tableau plus large que la page : ses cellules passent à la ligne, il garde la largeur
    // de la page, et le curseur reste dans ses cellules.
    let long = "mot ".repeat(120);
    let note = format!("avant\n\n| A | B |\n| --- | --- |\n| {long}| fin |\n\napres\n");
    let state = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).table_state().unwrap());
    let set_wide = |cx: &mut gpui::VisualTestContext, wide: Wide| {
        shell.update(cx, |s, cx| {
            s.theme = Theme { wide, ..s.theme };
            let theme = s.theme;
            s.editor.update(cx, |e, cx| e.set_theme(theme, cx));
        });
    };
    // Dans la colonne du texte : les cellules s'y partagent la largeur.
    set_wide(cx, Wide::Column);
    j.load(cx, &note, note.find("fin").unwrap());
    let (dx, tall, frame, page, _) = state(cx);
    assert!(dx == px(0.) && tall > 3 && frame.size.width <= page, "{dx:?} {tall} {frame:?} {page:?}");
    // Trop de colonnes pour tenir même à leur largeur minimale : le tableau défile, et suit
    // le curseur ; la molette horizontale le fait défiler aussi.
    let cols = 30;
    let head = format!("|{}\n|{}\n|{}\n", " colonne |".repeat(cols), " --- |".repeat(cols), " texte |".repeat(cols));
    j.load(cx, &head, head.rfind("texte").unwrap());
    let (dx, tall, frame, page, _) = state(cx);
    assert!(dx > px(0.) && frame.size.width <= page, "{dx:?} {tall} {frame:?} {page:?}");
    j.load(cx, &head, 0);
    assert_eq!(state(cx).0, px(0.));
    let on_table = shell.read_with(cx, |s, cx| s.editor.read(cx).point_of(head.find("colonne").unwrap()).unwrap());
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: on_table,
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(-90.), px(0.))),
        modifiers: gpui::Modifiers::none(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    cx.run_until_parked();
    assert_eq!(state(cx).0, px(90.));
    // Centré (le réglage par défaut) : le tableau sort de la colonne des deux côtés, sans dépasser
    // la fenêtre ; son bord gauche ne bouge pas avec la mise en page des autres lignes.
    set_wide(cx, Wide::Centered);
    j.load(cx, &note, note.find("fin").unwrap());
    let (dx, _, frame, page, view) = state(cx);
    assert!(dx == px(0.) && frame.size.width > page && frame.size.width <= view.size.width - px(48.), "{dx:?} {frame:?} {page:?} {view:?}");
    assert!((frame.left() - view.left() - (view.size.width - frame.size.width) / 2.).abs() < px(1.), "{frame:?} {view:?}");
    // Aux limites : fenêtre étroite, colonne pleine ou étroite, texte très petit ou très grand. Le tableau reste dans
    // la fenêtre avec ses marges, le texte aussi, et le texte n'est jamais plus large que ce que le réglage permet.
    let whole = cx.update(|window, _| window.viewport_size());
    for (width, page, zoom) in [(420., Page::Normal, 16.), (420., Page::Full, 16.), (1600., Page::Narrow, 16.), (1600., Page::Full, 32.), (300., Page::Normal, 32.), (1600., Page::Wide, 11.)] {
        cx.simulate_resize(size(px(width), px(600.)));
        shell.update(cx, |s, cx| {
            s.theme = Theme { page, size: zoom, wide: Wide::Centered, ..s.theme };
            let theme = s.theme;
            s.editor.update(cx, |e, cx| e.set_theme(theme, cx));
        });
        j.load(cx, &note, note.find("fin").unwrap());
        let (_, _, frame, text, view) = state(cx);
        let what = format!("fenêtre {width}, {page:?}, corps {zoom} : cadre {frame:?}, texte {text:?}, vue {view:?}");
        assert!(frame.left() >= view.left() + px(23.) && frame.right() <= view.right() - px(23.) || frame.size.width <= text, "{what}");
        assert!(text <= (view.size.width - px(48.)).max(px(40.)) && text <= page.measure(zoom).max(px(40.)), "{what}");
        assert!(frame.size.width >= px(40.).min(view.size.width - px(48.)), "{what}");
    }
    cx.simulate_resize(whole);
    shell.update(cx, |s, cx| {
        s.theme = Theme { page: Page::Normal, size: 16., ..s.theme };
        let theme = s.theme;
        s.editor.update(cx, |e, cx| e.set_theme(theme, cx));
    });
    // Un petit tableau garde sa taille, quel que soit le réglage.
    j.load(cx, "| a | b |\n| - | - |\n| c | d |\n", 0);
    let (_, _, frame, page, _) = state(cx);
    assert!(frame.size.width < page / 2., "{frame:?} {page:?}");
    set_wide(cx, Wide::Centered);

    // Coller dans un tableau : une cellule garde une seule ligne et ses `|` ; des
    // cellules de tableur remplissent la grille ; le tableau reste aligné.
    let small = "| Nom | Âge |\n| --- | --- |\n| Léa | 31 |\n";
    let paste = |cx: &mut gpui::VisualTestContext, clip: &str| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(clip.to_string()));
        cx.simulate_keystrokes("secondary-v");
    };
    let cell = small.find("Léa").unwrap();
    j.load(cx, small, cell);
    paste(cx, "Zoé\nBob |x");
    assert_eq!(text(cx), "| Nom            | Âge |\n| -------------- | --- |\n| Zoé Bob \\|xLéa | 31  |\n");
    j.load(cx, small, cell);
    paste(cx, "x\ty\nz\tw\n");
    assert_eq!(text(cx), "| Nom | Âge |\n| --- | --- |\n| x   | y   |\n| z   | w   |\n");
    j.load(cx, small, small.find("31").unwrap());
    paste(cx, "1\t2\t3");
    assert_eq!(text(cx), "| Nom | Âge |     |     |\n| --- | --- | --- | --- |\n| Léa | 1   | 2   | 3   |\n");
    // Un tableau Markdown copié ailleurs se recolle cellule par cellule.
    j.load(cx, small, cell);
    paste(cx, "| a | b |\n| :-- | --: |\n| c | d |");
    assert_eq!(text(cx), "| Nom | Âge |\n| --- | --- |\n| a   | b   |\n| c   | d   |\n");
    // Couper dans une cellule réaligne aussi.
    j.load(cx, small, cell);
    cx.simulate_keystrokes("shift-right shift-right secondary-x");
    assert_eq!(text(cx), "| Nom | Âge |\n| --- | --- |\n| a   | 31  |\n");

    // Coller n'importe quoi n'importe où dans un tableau : il reste une grille régulière.
    let clips = [
        "", "x", "a|b", "a\nb", "a\tb", "a\tb\nc\td\ne\tf", "| x | y |\n| - | - |\n| 1 | 2 |", "日本語\t😀", "  \n",
        "\t\t", "ligne1\r\nligne2", &"très long ".repeat(12), "[[a|b]]", "`x|y`", "\\|", "|", "||", "| ", "a\tb\t\n", "\n\n\n",
        "| a | b | c | d |", "# titre\n- liste\n> citation",
    ];
    let grid = "| Nom | Âge |\n| --- | --- |\n| Léa | 31 |\n| Max |  |\n";
    let mut cases = 0;
    for at in (0..grid.len()).step_by(2).filter(|&i| grid.is_char_boundary(i)) {
        for clip in clips {
            j.load(cx, grid, at);
            paste(cx, clip);
            let table = text(cx);
            let counts: Vec<usize> = table.lines().map(|l| markdown::cells(l).len()).collect();
            assert!(
                table.lines().all(|l| l.starts_with('|')) && counts.iter().all(|&n| n == counts[0]),
                "{clip:?} collé à {at} : {table:?}"
            );
            cases += 1;
        }
    }
    assert!(cases > 500);

    // Le curseur va de cellule en cellule sans s'arrêter sur les `|`, saute la ligne de
    // tirets, et effacer ne fusionne jamais deux cellules.
    let caret = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).caret());
    let t = "| Nom | Âge  |\n| --- | ---- |\n| Léa | a\\|b |\n| Max | 7    |\n";
    let at = |needle: &str| t.find(needle).unwrap();
    j.load(cx, t, at("Léa") + "Léa".len());
    cx.simulate_keystrokes("right");
    assert_eq!(caret(cx), at("a\\|b"));
    // `\|` s'affiche `|` : une flèche le franchit d'un coup.
    cx.simulate_keystrokes("right right");
    assert_eq!(caret(cx), at("a\\|b") + 3);
    cx.simulate_keystrokes("left left");
    assert_eq!(caret(cx), at("a\\|b"));
    cx.simulate_keystrokes("end");
    assert_eq!(caret(cx), at("a\\|b") + 4);
    // Au bout d'une ligne, la première cellule de la suivante ; au bout du tableau, la ligne d'après.
    cx.simulate_keystrokes("right");
    assert_eq!(caret(cx), at("Max"));
    cx.simulate_keystrokes("home left");
    assert_eq!(caret(cx), at("a\\|b") + 4);
    j.load(cx, t, at("Léa"));
    cx.simulate_keystrokes("left");
    assert_eq!(caret(cx), at("Âge") + "Âge".len(), "la ligne de tirets se saute");
    cx.simulate_keystrokes("down");
    assert!((at("a\\|b")..=at("a\\|b") + 4).contains(&caret(cx)), "descend dans la colonne, tirets sautés : {}", caret(cx));
    j.load(cx, t, t.len() - 1);
    cx.simulate_keystrokes("end right");
    assert_eq!(caret(cx), t.len());
    // Effacer s'arrête au bord de la cellule ; un `\|` part d'un bloc.
    j.load(cx, t, at("Léa"));
    cx.simulate_keystrokes("backspace");
    assert_eq!(text(cx), t);
    j.load(cx, t, at("a\\|b") + 3);
    cx.simulate_keystrokes("backspace");
    assert_eq!(text(cx), "| Nom | Âge |\n| --- | --- |\n| Léa | ab  |\n| Max | 7   |\n");
    j.load(cx, t, at("Léa") + "Léa".len());
    cx.simulate_keystrokes("delete");
    assert_eq!(text(cx), t);

    // Une cellule trop longue passe à la ligne : les flèches la parcourent ligne par ligne
    // (dans la colonne du texte, où elle est plus étroite).
    set_wide(cx, Wide::Column);
    let long = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega ".repeat(3);
    let wrapped = format!("| A | B |\n| --- | --- |\n| {long}| fin |\n| z | z |\n");
    let start = wrapped.find("alpha").unwrap();
    j.load(cx, &wrapped, start);
    let rows_of = |cx: &mut gpui::VisualTestContext, i: usize| shell.read_with(cx, |s, cx| s.editor.read(cx).point_of(i).unwrap());
    let (top, below) = (rows_of(cx, start).y, rows_of(cx, start + 140).y);
    assert!(below > top + px(30.), "la cellule passe à la ligne : {top:?} {below:?}");
    cx.simulate_keystrokes("down");
    let first = caret(cx);
    assert!(first > start && first < start + 80 && rows_of(cx, first).y > top, "{first}");
    cx.simulate_keystrokes("up");
    let back = caret(cx);
    assert!(back < start + 40 && rows_of(cx, back).y == top);
    // Un clic sur une ligne coupée place le curseur à cet endroit.
    let spot = rows_of(cx, start + 140) + gpui::point(px(2.), px(0.));
    cx.simulate_click(spot, gpui::Modifiers::none());
    assert!(caret(cx).abs_diff(start + 140) <= 1, "{} {}", caret(cx), start + 140);
    // Sélectionner d'une cellule à l'autre ne prend que le texte des cellules.
    j.load(cx, t, at("Léa"));
    cx.simulate_keystrokes("shift-right shift-right shift-right shift-right shift-right");
    assert_eq!(shell.read_with(cx, |s, cx| s.editor.read(cx).selected().to_string()), "Léa | a");

    // La ligne de tirets ne se voit pas, sauf sous le curseur ; un raccourci règle l'alignement.
    let hidden = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).hidden_rows());
    let small = "| Nom | Âge |\n| --- | --- |\n| Léa | 31  |\n";
    j.load(cx, small, 0);
    assert_eq!(hidden(cx), 1);
    j.load(cx, small, small.find("---").unwrap());
    assert_eq!(hidden(cx), 0);
    j.load(cx, small, small.find("Âge").unwrap());
    cx.simulate_keystrokes("secondary-shift-r");
    assert_eq!(text(cx), "| Nom | Âge |\n| --- | --: |\n| Léa | 31  |\n");
    cx.simulate_keystrokes("secondary-shift-e");
    assert_eq!(text(cx), "| Nom | Âge |\n| --- | :-: |\n| Léa | 31  |\n");
    cx.simulate_keystrokes("secondary-shift-l");
    assert_eq!(text(cx), "| Nom | Âge |\n| --- | :-- |\n| Léa | 31  |\n");

    // Largeur d'affichage : un idéogramme compte pour deux colonnes dans le texte aligné.
    j.load(cx, "| a |\n| --- |\n| 日本語 |\n", 3);
    cx.simulate_input("x");
    assert_eq!(text(cx), "| ax     |\n| ------ |\n| 日本語 |\n");

    // Coller un tableau Markdown, ou des cellules de page web ou de tableur, hors d'un tableau.
    let clip = "| a | b |\n|:--|--:|\n| longue cellule | 2 |";
    let table = "| a              | b   |\n| :------------- | --: |\n| longue cellule | 2   |";
    j.load(cx, "avant\n\napres\n", 7);
    paste(cx, clip);
    assert_eq!(text(cx), format!("avant\n\n{table}\n\napres\n"), "{}", text(cx));
    // Des cellules séparées par des tabulations : la première ligne est l'en-tête.
    j.load(cx, "avant", 5);
    paste(cx, "Nom\tÂge\nLéa\t31\n");
    assert_eq!(text(cx), "avant\n\n| Nom | Âge |\n| --- | --- |\n| Léa | 31  |", "{}", text(cx));
    // Du code indenté par des tabulations, ou une seule ligne, reste du texte.
    j.load(cx, "", 0);
    paste(cx, "\tfn a()\n\t\tx");
    assert_eq!(text(cx), "\tfn a()\n\t\tx");

    // Des centaines de gestes au hasard (graine fixe) dans des tableaux de toute sorte : aucun
    // plantage, le curseur reste sur un bord de caractère, et un tableau en forme reste régulier.
    // `BREF_FUZZ_SEED=n cargo test end_to_end` essaie une autre suite de gestes.
    let mut seed = std::env::var("BREF_FUZZ_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0x9E3779B97F4A7C15u64);
    let mut roll = move |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    let starts = [
        "| a | b |\n| --- | --- |\n| c | d |\n",
        "avant\n\n| 日本語 | 😀 émoji |\n| :-: | --: |\n| a \\| b | `x` **y** |\n\napres",
        "|\n| a\n| a | b | c\n",
        "| seul |\n",
        "| H1 | H2 |\n| --- | --- |\n| texte très long qui passe à la ligne dans sa cellule étroite | 1 |\n| | |\n",
        "```\n| pas | un tableau |\n```\n| mais | si |\n",
    ];
    let keys = [
        "left", "right", "up", "down", "home", "end", "backspace", "delete", "tab", "shift-tab", "enter",
        "shift-left", "shift-right", "shift-down", "shift-up", "secondary-left", "secondary-right", "secondary-z",
        "secondary-shift-z", "secondary-backspace", "secondary-home", "secondary-end", "pageup", "pagedown",
    ];
    let typed = ["a", "é", "日本", "😀", " ", "|", "\\|", "x y z", "-", ":", "[[n|a]]", "`c|d`"];
    let mut gestures = 0;
    for start in starts {
        for round in 0..3 {
            j.load(cx, start, if round % 2 == 0 { 0 } else { start.len() / 2 });
            for _ in 0..30 {
                let before = text(cx);
                // Une sélection qui sort du tableau remplace aussi ce qu'elle touche hors de lui.
                let span = shell.read_with(cx, |s, cx| s.editor.read(cx).span());
                let leaves = {
                    let (a, b) = (before[..span.start].rfind('\n').map_or(0, |i| i + 1), before[span.end..].find('\n').map_or(before.len(), |i| span.end + i));
                    !span.is_empty() && before[a..b].split('\n').any(|l| !l.trim_start().starts_with('|'))
                };
                let step = match roll(10) {
                    0..=4 => {
                        let key = keys[roll(keys.len())];
                        cx.simulate_keystrokes(key);
                        format!("touche {key}")
                    }
                    5..=7 => {
                        let input = typed[roll(typed.len())];
                        cx.simulate_input(input);
                        format!("saisie {input:?}")
                    }
                    _ => {
                        let clip = clips[roll(clips.len())];
                        paste(cx, clip);
                        format!("collage {clip:?}")
                    }
                };
                gestures += 1;
                let (now, at) = (text(cx), caret(cx));
                assert!(at <= now.len() && now.is_char_boundary(at), "curseur {at} dans {now:?}");
                cx.run_until_parked();
                // Chaque tableau qui était déjà en forme (ligne de tirets en deuxième) garde le même
                // nombre de cellules partout : taper ou coller n'en défait aucun.
                let formed = |text: &str| -> Vec<(usize, Vec<usize>)> {
                    let lines: Vec<&str> = text.lines().collect();
                    let is_row = |l: &str| l.trim_start().starts_with('|');
                    let dashes = |l: &str| markdown::cells(l).iter().all(|r| l[r.clone()].contains('-') && l[r.clone()].chars().all(|c| matches!(c, '-' | ':')));
                    let mut fence = false;
                    let mut found = Vec::new();
                    for (i, l) in lines.iter().enumerate() {
                        fence ^= l.trim_start().starts_with("```");
                        let top = !fence && is_row(l) && (i == 0 || !is_row(lines[i - 1]));
                        if top && lines.get(i + 1).is_some_and(|d| is_row(d) && dashes(d)) {
                            let counts = lines[i..].iter().take_while(|l| is_row(l)).map(|l| markdown::cells(l).len()).filter(|&n| n > 0);
                            found.push((i, counts.collect()));
                        }
                    }
                    found
                };
                let steady = |counts: &[usize]| counts.iter().all(|&n| n == counts[0]);
                let was: Vec<usize> = formed(&before).into_iter().filter(|(_, c)| steady(c)).map(|(i, _)| i).collect();
                for (i, counts) in formed(&now) {
                    assert!(steady(&counts) || !was.contains(&i) || leaves || step.ends_with("-z"), "{counts:?} après {step} (curseur {at}, sélection {span:?}) : {before:?} -> {now:?}");
                }
            }
        }
    }
    assert!(gestures > 500);

    // `/meta` (ou `/tags`, `/alias`) pose l'en-tête YAML en tête de note, où qu'on soit, le
    // curseur entre les crochets des tags ; redemandé, il y ramène sans en poser un second.
    j.load(cx, "# Titre\n\ntexte ", "# Titre\n\ntexte ".len());
    cx.simulate_input("/tags");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("projet");
    assert_eq!(text(cx), "---\ntags: [projet]\naliases: []\n---\n# Titre\n\ntexte ");
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input("/meta");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("x");
    assert_eq!(text(cx), "---\ntags: [xprojet]\naliases: []\n---\n# Titre\n\ntexte ");

    // En-tête YAML ouvert au clavier : tant qu'il n'est pas fermé, sa première ligne reste une règle.
    j.load(cx, "---\na: 1\n", 9);
    assert_eq!(j.kind(cx, 0), Some(markdown::Kind::Rule));
    cx.simulate_input("---");
    cx.run_until_parked();
    assert_eq!((j.kind(cx, 0), j.kind(cx, 4), j.kind(cx, 9)), (Some(markdown::Kind::Meta), Some(markdown::Kind::Meta), Some(markdown::Kind::Meta)));
}
