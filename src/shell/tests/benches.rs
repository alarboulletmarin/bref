//! Measures, not checks: run by hand with `cargo test --release --locked -- --ignored --nocapture <name>`.

use super::*;

/// Ouvre chaque fichier du dossier `BREF_CSV_DIR` dans le tableau (copié, rien n'est modifié chez
/// l'utilisateur), chronomètre l'ouverture, puis enchaîne des gestes au hasard : un plantage se voit ici.
/// `BREF_CSV_DIR=<dossier> cargo test --release --locked sheet_fuzz -- --ignored --nocapture`.
#[gpui::test]
#[ignore]
fn sheet_fuzz(cx: &mut TestAppContext) {
    let Ok(from) = std::env::var("BREF_CSV_DIR") else { return };
    let root = std::env::temp_dir().join(format!("bref-fuzz-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    let mut files = Vec::new();
    for entry in fs::read_dir(&from).unwrap().flatten() {
        let to = root.join(entry.file_name());
        fs::copy(entry.path(), &to).unwrap();
        if vault::is_table(&to) {
            files.push(to);
        }
    }
    files.sort();
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx.run_until_parked();
    let mut seed = 0x2545F4914F6CDD1Du64;
    let mut next = move |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    let keys = "left right up down shift-left shift-right shift-up shift-down pageup pagedown home end secondary-home secondary-end secondary-up secondary-down tab shift-tab enter f2 escape backspace delete secondary-c secondary-x secondary-v secondary-a secondary-z secondary-shift-z secondary-enter secondary-shift-enter secondary-delete".split(' ').collect::<Vec<_>>();
    let texts = ["a", "é", "x y", "€", "☃", "😀e\u{301}", ",", "\"", "12"];
    for file in files {
        let start = Instant::now();
        shell.update_in(cx, |s, window, cx| {
            s.open_note(&file, cx);
            if let Some(sheet) = s.shown_sheet() {
                window.focus(&sheet.focus_handle(cx));
            }
        });
        cx.run_until_parked();
        println!("{:?} : ouverture + affichage {:?}", file.file_name().unwrap(), start.elapsed());
        let sheet = shell.read_with(cx, |s, _| s.shown_sheet().expect("tableau affiché"));
        let mut worst = Duration::ZERO;
        for step in 0..600 {
            let begin = Instant::now();
            match next(10) {
                0..=4 => cx.simulate_keystrokes(keys[next(keys.len())]),
                5 | 6 => cx.simulate_input(texts[next(texts.len())]),
                7 => {
                    let at = point(px(330. + next(520) as f32), px(60. + next(640) as f32));
                    let mods = if next(2) == 0 { gpui::Modifiers::none() } else { gpui::Modifiers::shift() };
                    cx.simulate_mouse_down(at, gpui::MouseButton::Left, mods);
                    let to = point(px(330. + next(520) as f32), px(60. + next(640) as f32));
                    cx.simulate_mouse_move(to, gpui::MouseButton::Left, mods);
                    cx.simulate_mouse_up(to, gpui::MouseButton::Left, mods);
                }
                8 => cx.simulate_event(gpui::ScrollWheelEvent {
                    position: point(px(700.), px(300.)),
                    delta: gpui::ScrollDelta::Pixels(point(px(next(200) as f32 - 100.), px(next(4000) as f32 - 2000.))),
                    ..Default::default()
                }),
                _ => match next(5) {
                    0 => sheet.update(cx, |s, cx| s.toggle_header(cx)),
                    1 => sheet.update(cx, |s, cx| s.copy_as(table::Style::Json, cx)),
                    2 => sheet.update(cx, |s, cx| s.reformat(Some(table::Format { delimiter: table::DELIMITERS[next(6)], ..s.format() }), cx)),
                    3 => sheet.update(cx, |s, cx| s.export(table::Style::Markdown, cx)),
                    _ => sheet.update(cx, |s, cx| s.reformat(Some(table::Format { encoding: table::Encoding::ALL[next(6)], ..s.format() }), cx)),
                },
            }
            cx.run_until_parked();
            worst = worst.max(begin.elapsed());
            assert!(step < 10_000);
        }
        println!("   geste le plus lent : {worst:?}");
    }
    fs::remove_dir_all(&root).unwrap();
}

/// Un gros CSV transformé : copie et export dans chaque format, ouverture de la note Markdown obtenue,
/// collage du texte dans une note. `BREF_CSV=<fichier> cargo test --release --locked sheet_convert -- --ignored --nocapture`.
#[gpui::test]
#[ignore]
fn sheet_convert(cx: &mut TestAppContext) {
    let Ok(from) = std::env::var("BREF_CSV") else { return };
    let root = std::env::temp_dir().join(format!("bref-convert-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    let file = root.join("gros.csv");
    fs::copy(&from, &file).unwrap();
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx.run_until_parked();
    shell.update_in(cx, |s, window, cx| {
        s.open_note(&file, cx);
        window.focus(&s.shown_sheet().unwrap().focus_handle(cx));
    });
    cx.run_until_parked();
    let sheet = shell.read_with(cx, |s, _| s.shown_sheet().unwrap());
    cx.simulate_keystrokes("secondary-a");
    for (name, style) in [("TSV", table::Style::Tsv), ("CSV", table::Style::Csv(b',')), ("Markdown", table::Style::Markdown), ("JSON", table::Style::Json)] {
        let start = Instant::now();
        sheet.update(cx, |s, cx| s.copy_as(style, cx));
        let copied = start.elapsed();
        let len = cx.read_from_clipboard().and_then(|c| c.text()).map_or(0, |t| t.len());
        let start = Instant::now();
        sheet.update(cx, |s, cx| s.export(style, cx));
        cx.run_until_parked();
        println!("{name:9} copie {copied:?} ({} Mo) · export {:?}", len >> 20, start.elapsed());
    }
    let exported = root.join(format!("gros{}.md", tr(" (selection)", " (sélection)")));
    println!("note exportée : {} Mo", fs::metadata(&exported).map_or(0, |m| m.len()) >> 20);
    let start = Instant::now();
    shell.update(cx, |s, cx| {
        let (notes, dirs, images) = vault::scan(&root);
        s.sync(notes, dirs, images, cx);
    });
    println!("rescan du coffre avec cette note : {:?}", start.elapsed());
    let start = Instant::now();
    shell.update_in(cx, |s, window, cx| {
        s.open_note(&exported, cx);
        window.focus(&s.pane().editor.focus_handle(cx));
    });
    cx.run_until_parked();
    println!("ouverture de la note Markdown : {:?}", start.elapsed());
    let start = Instant::now();
    cx.simulate_input("a");
    cx.run_until_parked();
    println!("une frappe dedans : {:?}", start.elapsed());
    let start = Instant::now();
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    println!("flèche bas dedans : {:?}", start.elapsed());
    // Coller le TSV d'un gros tableau dans une note neuve.
    let tsv: String = (0..300_000).map(|i| format!("{i}\tProduit {}\t{}\n", i % 977, i % 99)).collect();
    cx.write_to_clipboard(ClipboardItem::new_string(tsv));
    shell.update_in(cx, |s, window, cx| {
        s.new_note(String::new(), cx);
        window.focus(&s.pane().editor.focus_handle(cx));
    });
    let start = Instant::now();
    cx.simulate_keystrokes("secondary-v");
    cx.run_until_parked();
    println!("coller 300 000 lignes de TSV dans une note : {:?}", start.elapsed());
    for what in ["a", "down", "up", "enter"] {
        let start = Instant::now();
        if what.len() == 1 { cx.simulate_input(what) } else { cx.simulate_keystrokes(what) }
        cx.run_until_parked();
        println!("   dans la note collée, {what:6} : {:?}", start.elapsed());
    }
    fs::remove_dir_all(&root).unwrap();
}

/// Coût d'une note qui contient un tableau Markdown de n lignes.
#[gpui::test]
#[ignore]
fn md_table_scale(cx: &mut TestAppContext) {
    // Les tests refont toute la mise en page après chaque reprise, pour la vérifier : pas ici, on mesure.
    unsafe { std::env::set_var("BREF_NO_COMPARE", "1") };
    let root = std::env::temp_dir().join(format!("bref-mdscale-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx.run_until_parked();
    for n in std::env::var("BREF_ROWS").ok().map_or(vec![500usize, 1000, 2000, 5000, 10000, 20000, 50000], |v| v.split(',').filter_map(|n| n.parse().ok()).collect()) {
        let rows: String = (0..n).map(|i| format!("| {i} | Produit {} | {} | FR | note, {i} |\n", i % 977, i % 99)).collect();
        let file = root.join(format!("t{n}.md"));
        fs::write(&file, format!("| id | produit | qte | pays | commentaire |\n| --- | --- | --- | --- | --- |\n{rows}")).unwrap();
        let start = Instant::now();
        shell.update_in(cx, |s, window, cx| {
            s.open_note(&file, cx);
            window.focus(&s.pane().editor.focus_handle(cx));
        });
        cx.run_until_parked();
        let opened = start.elapsed();
        // Au milieu du tableau, loin des lignes qui fixent ses colonnes.
        shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| {
            let at = e.text().len() / 2;
            e.place_cursor(at, cx)
        }));
        cx.run_until_parked();
        let start = Instant::now();
        cx.simulate_input("a");
        cx.run_until_parked();
        println!("{n:6} lignes : ouverture {opened:?} · une frappe {:?}", start.elapsed());
        let start = Instant::now();
        for _ in 0..20 {
            cx.simulate_event(gpui::ScrollWheelEvent {
                position: point(px(700.), px(300.)),
                delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-40.))),
                ..Default::default()
            });
            cx.run_until_parked();
        }
        println!("         défilement : {:?} par geste", start.elapsed() / 20);
        if std::env::var("BREF_TYPING").is_ok() {
            let start = Instant::now();
            for _ in 0..30 {
                cx.simulate_input("a");
                cx.run_until_parked();
            }
            println!("         30 frappes de suite : {:?} par frappe", start.elapsed() / 30);
            let start = Instant::now();
            for _ in 0..30 {
                cx.simulate_input(" ");
                cx.run_until_parked();
            }
            println!("         30 espaces de suite : {:?} par espace", start.elapsed() / 30);
            let start = Instant::now();
            cx.simulate_keystrokes("secondary-z");
            cx.run_until_parked();
            println!("         annuler : {:?}", start.elapsed());
            let start = Instant::now();
            shell.update(cx, |s, cx| s.flush(cx));
            println!("         enregistrement de la note ({} Mo) : {:?}", fs::metadata(&file).map_or(0, |m| m.len()) >> 20, start.elapsed());
        }
    }
    fs::remove_dir_all(&root).unwrap();
}

/// Frappes dans une grosse note, pour un profileur : `cargo test --release --locked -- --ignored profile_typing`.
#[gpui::test]
#[ignore]
fn profile_typing(cx: &mut TestAppContext) {
    // Les tests refont toute la mise en page après chaque reprise, pour la vérifier : pas ici, on mesure.
    unsafe { std::env::set_var("BREF_NO_COMPARE", "1") };
    let root = std::env::temp_dir().join(format!("bref-profile-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx.run_until_parked();
    // `BREF_PROFILE_LINES=n` : longueur de la note (5000 par défaut).
    let lines = std::env::var("BREF_PROFILE_LINES").ok().and_then(|n| n.parse().ok()).unwrap_or(5000);
    let big: String = if std::env::var("BREF_PROFILE_TABLE").is_ok() {
        // Un tableau de `lines` lignes.
        let rows: String = (0..lines)
            .map(|j| format!("| Ligne {j} | texte moyen avec **gras** et `code` {j} | {}| [[Note {}]] | x | y |\n", "mot ".repeat(j % 30), j))
            .collect();
        format!("| A | B | C | D | E | F |\n| --- | --- | --- | --- | --- | --- |\n{rows}")
    } else {
        (0..lines).map(|j| format!("Ligne {j} avec **gras**, `code`, [[Note {}]] et #tag{} -> fin.\n", j % 2000, j % 12)).collect()
    };
    shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.load(format!("# Grosse\n\n{big}"), 100, cx)));
    cx.run_until_parked();
    for _ in 0..300 {
        cx.simulate_input("a");
        cx.run_until_parked();
    }
    println!("mises en page (complètes, reprises, dans un gros tableau) : {:?}", shell.read_with(cx, |s, cx| s.pane().editor.read(cx).layouts()));
    fs::remove_dir_all(&root).unwrap();
}

/// Coffre synthétique, toujours le même : `notes` notes de 30 lignes réparties dans 40
/// dossiers, liées entre elles et étiquetées, plus `Grosse.md`, une note de 5 000 lignes.
fn synthetic_vault(root: &Path, notes: usize) {
    for i in 0..notes {
        let dir = root.join(format!("d{}", i % 40));
        fs::create_dir_all(&dir).unwrap();
        let body: String = (0..30).map(|j| format!("Ligne {j} vers [[Note {}]] #tag{}\n", (i + j) % notes, j % 12)).collect();
        fs::write(dir.join(format!("Note {i}.md")), format!("# Note {i}\n\n{body}")).unwrap();
    }
    let big: String =
        (0..5000).map(|j| format!("Ligne {j} avec **gras**, `code`, [[Note {}]] et #tag{} -> fin.\n", j % notes, j % 12)).collect();
    fs::write(root.join("Grosse.md"), format!("# Grosse\n\n{big}")).unwrap();
}

/// Mémoire résidente du processus en Mo (Linux seulement).
fn resident_mb() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let kb: u64 = status.lines().find_map(|l| l.strip_prefix("VmRSS:"))?.trim().trim_end_matches("kB").trim().parse().ok()?;
    Some(kb / 1024)
}

/// Temps pour un coffre de 1 000, 5 000 puis 20 000 notes : scan, nouveau scan sans
/// changement, empreinte, démarrage de l'app, ouverture et frappe dans une note de
/// 5 000 lignes. `BREF_BENCH_NOTES=500,3000` change les tailles. Hors de la suite
/// courante : `cargo test --release --locked -- --ignored --nocapture bench_scale`.
#[gpui::test]
#[ignore]
fn bench_scale(cx: &mut TestAppContext) {
    // Les tests refont toute la mise en page après chaque reprise, pour la vérifier : pas ici, on mesure.
    unsafe { std::env::set_var("BREF_NO_COMPARE", "1") };
    let sizes: Vec<usize> = std::env::var("BREF_BENCH_NOTES")
        .map(|v| v.split(',').filter_map(|n| n.trim().parse().ok()).collect())
        .unwrap_or_else(|_| vec![1000, 5000, 20000]);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let ms = |d: Duration| format!("{:.1}", d.as_secs_f64() * 1000.);
    let mut rows = Vec::new();
    for notes in sizes {
        let root = std::env::temp_dir().join(format!("bref-scale-{notes}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        synthetic_vault(&root, notes);
        let _config = isolated_config(&root);
        vault::save_layout("tree split 260 0");

        let start = Instant::now();
        let scanned = vault::scan(&root);
        let scan = start.elapsed();
        let start = Instant::now();
        let again = vault::rescan(&root, &scanned.0);
        let rescan = start.elapsed();
        assert_eq!(again.0.len(), notes + 1);
        let start = Instant::now();
        vault::fingerprint(&root);
        let fingerprint = start.elapsed();

        let start = Instant::now();
        let (shell, cx) = cx.add_window_view({
            let root = root.clone();
            |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
        });
        cx.run_until_parked();
        let startup = start.elapsed();
        let start = Instant::now();
        shell.update(cx, |s, cx| s.open_note(&root.join("Grosse.md"), cx));
        cx.run_until_parked();
        let open = start.elapsed();
        let start = Instant::now();
        for _ in 0..20 {
            cx.simulate_input("a");
            cx.run_until_parked();
        }
        let typing = start.elapsed() / 20;
        let memory = resident_mb().map_or("?".into(), |m| m.to_string());
        // Enregistrer la grosse note où l'on vient de taper : ce que coûte un enregistrement.
        let start = Instant::now();
        shell.update(cx, |s, cx| s.flush(cx));
        let saved = start.elapsed();
        // La palette sur tout le coffre : l'ouvrir, puis y taper une lettre.
        let start = Instant::now();
        cx.simulate_keystrokes("secondary-p");
        cx.run_until_parked();
        let palette = start.elapsed();
        let start = Instant::now();
        cx.simulate_input("n");
        cx.run_until_parked();
        let letter = start.elapsed();
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        // Une note écrite par un autre programme : ce que le fil principal y passe (copie
        // des notes pour la relecture, puis `sync` et le dessin), relecture non comprise.
        fs::write(root.join("d0").join("Surprise.md"), "# Surprise\n\n[[Note 1]]\n").unwrap();
        let start = Instant::now();
        let known = shell.read_with(cx, |s, _| s.notes.clone());
        let copy = start.elapsed();
        let (found, dirs, images) = vault::rescan(&root, &known);
        let start = Instant::now();
        shell.update(cx, |s, cx| s.sync(found, dirs, images, cx));
        let synced = start.elapsed();
        // Une touche sans effet sur le texte : l'image suivante, panneau compris.
        cx.simulate_keystrokes("right");
        cx.run_until_parked();
        let outside = copy + start.elapsed();
        let _ = synced;
        rows.push(format!("| {notes} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {memory} |", ms(scan), ms(rescan), ms(fingerprint), ms(startup), ms(open), ms(typing), ms(saved), ms(palette), ms(letter), ms(outside)));
        // La fenêtre et son watcher partent avant le coffre suivant.
        cx.update(|_, cx| cx.windows().iter().for_each(|w| { w.update(cx, |_, window, _| window.remove_window()).ok(); }));
        cx.run_until_parked();
        fs::remove_dir_all(&root).unwrap();
    }
    println!("\n| notes | scan (ms) | rescan (ms) | empreinte (ms) | démarrage (ms) | ouverture 5000 l. (ms) | frappe 5000 l. (ms) | enregistrement 5000 l. (ms) | palette : ouvrir (ms) | palette : une lettre (ms) | note écrite ailleurs, fil principal (ms) | mémoire (Mo) |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
    rows.iter().for_each(|row| println!("{row}"));
}

/// Temps par geste sur un gros coffre et une grosse note. Hors de la suite
/// courante : `cargo test --release --locked -- --ignored --nocapture bench`.
#[gpui::test]
#[ignore]
fn bench(cx: &mut TestAppContext) {
    // Les tests refont toute la mise en page après chaque reprise, pour la vérifier : pas ici, on mesure.
    unsafe { std::env::set_var("BREF_NO_COMPARE", "1") };
    let root = std::env::temp_dir().join(format!("bref-bench-{}", std::process::id()));
    synthetic_vault(&root, 2000);
    let _config = isolated_config(&root);
    vault::save_layout("tree split 260 0");

    let start = Instant::now();
    let scanned = vault::scan(&root);
    println!("scan du coffre : {:?}", start.elapsed());
    let start = Instant::now();
    let again = vault::rescan(&root, &scanned.0);
    println!("nouveau scan, rien n'a changé : {:?}", start.elapsed());
    assert_eq!(again.0.len(), scanned.0.len());

    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx.run_until_parked();
    shell.update(cx, |s, cx| s.open_note(&root.join("Grosse.md"), cx));
    cx.run_until_parked();
    let time = |what: &str, cx: &mut gpui::VisualTestContext, act: &dyn Fn(&mut gpui::VisualTestContext)| {
        let start = Instant::now();
        for _ in 0..20 {
            act(cx);
            cx.run_until_parked();
        }
        println!("{what} : {:?} par geste", start.elapsed() / 20);
    };
    time("frappe", cx, &|cx| cx.simulate_input("a"));
    time("flèche bas", cx, &|cx| cx.simulate_keystrokes("down"));
    time("Entrée", cx, &|cx| cx.simulate_keystrokes("enter"));
    time("molette", cx, &|cx| {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: point(px(700.), px(300.)),
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-40.))),
            ..Default::default()
        })
    });
    time("mot puis annulation", cx, &|cx| {
        cx.simulate_input("mot ");
        cx.simulate_keystrokes("secondary-z");
    });
    // Selon la longueur de la note : le prix d'une frappe est un prix fixe, plus un peu par ligne.
    for lines in [300, 1000] {
        let body: String = (0..lines).map(|j| format!("Ligne {j} avec **gras**, `code`, [[Note {}]] et #tag{} -> fin.\n", j % 2000, j % 12)).collect();
        shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.load(format!("# Note\n\n{body}"), 100, cx)));
        cx.run_until_parked();
        time(&format!("frappe dans une note de {lines} lignes"), cx, &|cx| cx.simulate_input("a"));
    }
    // Le même geste sur une note courte : ce qui reste est le prix fixe d'une frappe.
    shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.load("# Courte\n\nun peu de texte\n".into(), 20, cx)));
    cx.run_until_parked();
    time("frappe dans une note courte", cx, &|cx| cx.simulate_input("a"));
    // Un grand tableau, dont les cellules passent à la ligne.
    let rows: String = (0..400)
        .map(|j| format!("| Ligne {j} | texte moyen avec **gras** et `code` {j} | {}| [[Note {}]] | x | y |\n", "mot ".repeat(j % 30), j))
        .collect();
    let table = format!("# Tableau\n\n| A | B | C | D | E | F |\n| --- | --- | --- | --- | --- | --- |\n{rows}");
    shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.load(table, 120, cx)));
    cx.run_until_parked();
    time("frappe dans un tableau de 400 lignes", cx, &|cx| cx.simulate_input("a"));
    time("flèche bas dans ce tableau", cx, &|cx| cx.simulate_keystrokes("down"));
    time("molette sur ce tableau", cx, &|cx| {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: point(px(700.), px(300.)),
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-40.))),
            ..Default::default()
        })
    });
    fs::remove_dir_all(&root).unwrap();
}
