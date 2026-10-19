//! Measures, not checks: run by hand with `cargo test --release --locked -- --ignored --nocapture <name>`.

use super::*;

/// Opens each file of the `BREF_CSV_DIR` folder in the table (copied, nothing of the user's
/// is changed), times the opening, then chains random gestures: a crash shows here.
/// `BREF_CSV_DIR=<folder> cargo test --release --locked sheet_fuzz -- --ignored --nocapture`.
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
        println!("{:?}: opening + display {:?}", file.file_name().unwrap(), start.elapsed());
        let sheet = shell.read_with(cx, |s, _| s.shown_sheet().expect("table shown"));
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
        println!("   slowest gesture: {worst:?}");
    }
    fs::remove_dir_all(&root).unwrap();
}

/// A big CSV transformed: copy and export in each format, opening of the resulting Markdown note,
/// pasting the text into a note. `BREF_CSV=<file> cargo test --release --locked sheet_convert -- --ignored --nocapture`.
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
        println!("{name:9} copy {copied:?} ({} MB) · export {:?}", len >> 20, start.elapsed());
    }
    let exported = root.join(format!("gros{}.md", tr(" (selection)", " (sélection)")));
    println!("exported note: {} MB", fs::metadata(&exported).map_or(0, |m| m.len()) >> 20);
    let start = Instant::now();
    shell.update(cx, |s, cx| {
        let (notes, dirs, images) = vault::scan(&root);
        s.sync(notes, dirs, images, cx);
    });
    println!("rescan of the vault with this note: {:?}", start.elapsed());
    let start = Instant::now();
    shell.update_in(cx, |s, window, cx| {
        s.open_note(&exported, cx);
        window.focus(&s.pane().editor.focus_handle(cx));
    });
    cx.run_until_parked();
    println!("opening the Markdown note: {:?}", start.elapsed());
    let start = Instant::now();
    cx.simulate_input("a");
    cx.run_until_parked();
    println!("one keystroke inside: {:?}", start.elapsed());
    let start = Instant::now();
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    println!("down arrow inside: {:?}", start.elapsed());
    // Paste the TSV of a big table into a fresh note.
    let tsv: String = (0..300_000).map(|i| format!("{i}\tProduit {}\t{}\n", i % 977, i % 99)).collect();
    cx.write_to_clipboard(ClipboardItem::new_string(tsv));
    shell.update_in(cx, |s, window, cx| {
        s.new_note(String::new(), cx);
        window.focus(&s.pane().editor.focus_handle(cx));
    });
    let start = Instant::now();
    cx.simulate_keystrokes("secondary-v");
    cx.run_until_parked();
    println!("pasting 300,000 TSV lines into a note: {:?}", start.elapsed());
    for what in ["a", "down", "up", "enter"] {
        let start = Instant::now();
        if what.len() == 1 { cx.simulate_input(what) } else { cx.simulate_keystrokes(what) }
        cx.run_until_parked();
        println!("   in the pasted note, {what:6}: {:?}", start.elapsed());
    }
    fs::remove_dir_all(&root).unwrap();
}

/// Cost of a note containing a Markdown table of n rows.
#[gpui::test]
#[ignore]
fn md_table_scale(cx: &mut TestAppContext) {
    // Tests redo the whole layout after each reuse, to check it: not here, we measure.
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
        // `BREF_FORMULAS=1`: a running total (`=sum(above)`) in the last column of each line.
        let last = |i: usize| if std::env::var("BREF_FORMULAS").is_ok() { "=sum(above)".to_string() } else { format!("note, {i}") };
        let rows: String = (0..n).map(|i| format!("| {i} | Produit {} | {} | FR | {} |\n", i % 977, i % 99, last(i))).collect();
        let file = root.join(format!("t{n}.md"));
        // `BREF_COMMENTS=1`: comments under the table, which each keystroke in it must keep up with.
        let notes = if std::env::var("BREF_COMMENTS").is_ok() { "\n{>>Row \"0\": a<<}\n\n{>>Column \"id\": b<<}\n\n{>>Table: c<<}\n" } else { "" };
        fs::write(&file, format!("| id | produit | qte | pays | commentaire |\n| --- | --- | --- | --- | --- |\n{rows}{notes}")).unwrap();
        let start = Instant::now();
        shell.update_in(cx, |s, window, cx| {
            s.open_note(&file, cx);
            window.focus(&s.pane().editor.focus_handle(cx));
        });
        cx.run_until_parked();
        let opened = start.elapsed();
        // In the middle of the table, far from the rows that set its columns.
        shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| {
            let at = e.text().len() / 2;
            e.place_cursor(at, cx)
        }));
        cx.run_until_parked();
        let start = Instant::now();
        cx.simulate_input("a");
        cx.run_until_parked();
        println!("{n:6} lines: opening {opened:?} · one keystroke {:?}", start.elapsed());
        let start = Instant::now();
        for _ in 0..20 {
            cx.simulate_event(gpui::ScrollWheelEvent {
                position: point(px(700.), px(300.)),
                delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-40.))),
                ..Default::default()
            });
            cx.run_until_parked();
        }
        println!("         scrolling: {:?} per gesture", start.elapsed() / 20);
        if std::env::var("BREF_TYPING").is_ok() {
            let start = Instant::now();
            for _ in 0..30 {
                cx.simulate_input("a");
                cx.run_until_parked();
            }
            println!("         30 keystrokes in a row: {:?} per keystroke", start.elapsed() / 30);
            let start = Instant::now();
            for _ in 0..30 {
                cx.simulate_input(" ");
                cx.run_until_parked();
            }
            println!("         30 spaces in a row: {:?} per space", start.elapsed() / 30);
            let start = Instant::now();
            cx.simulate_keystrokes("secondary-z");
            cx.run_until_parked();
            println!("         undo: {:?}", start.elapsed());
            let start = Instant::now();
            shell.update(cx, |s, cx| s.flush(cx));
            println!("         saving the note ({} MB): {:?}", fs::metadata(&file).map_or(0, |m| m.len()) >> 20, start.elapsed());
        }
    }
    fs::remove_dir_all(&root).unwrap();
}

/// Keystrokes in a big note, for a profiler: `cargo test --release --locked -- --ignored profile_typing`.
#[gpui::test]
#[ignore]
fn profile_typing(cx: &mut TestAppContext) {
    // Tests redo the whole layout after each reuse, to check it: not here, we measure.
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
    // `BREF_PROFILE_LINES=n`: length of the note (5000 by default).
    let lines = std::env::var("BREF_PROFILE_LINES").ok().and_then(|n| n.parse().ok()).unwrap_or(5000);
    let big: String = if std::env::var("BREF_PROFILE_TABLE").is_ok() {
        // A table of `lines` rows.
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
    println!("layouts (full, reused, in a big table): {:?}", shell.read_with(cx, |s, cx| s.pane().editor.read(cx).layouts()));
    fs::remove_dir_all(&root).unwrap();
}

/// Synthetic vault, always the same: `notes` notes of 30 lines spread over 40
/// folders, linked to each other and tagged, plus `Grosse.md`, a note of 5,000 lines.
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

/// Resident memory of the process in MB (Linux only).
fn resident_mb() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let kb: u64 = status.lines().find_map(|l| l.strip_prefix("VmRSS:"))?.trim().trim_end_matches("kB").trim().parse().ok()?;
    Some(kb / 1024)
}

/// Time for a vault of 1,000, 5,000 then 20,000 notes: scan, new scan without
/// change, fingerprint, app startup, opening and typing in a note of
/// 5,000 lines. `BREF_BENCH_NOTES=500,3000` changes the sizes. Outside the regular
/// suite: `cargo test --release --locked -- --ignored --nocapture bench_scale`.
#[gpui::test]
#[ignore]
fn bench_scale(cx: &mut TestAppContext) {
    // Tests redo the whole layout after each reuse, to check it: not here, we measure.
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
        // Save the big note just typed in: what a save costs.
        let start = Instant::now();
        shell.update(cx, |s, cx| s.flush(cx));
        let saved = start.elapsed();
        // The palette over the whole vault: open it, then type a letter in it.
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
        // A note written by another program: what the main thread spends on it (copy
        // of the notes for reloading, then `sync` and drawing), reloading not included.
        fs::write(root.join("d0").join("Surprise.md"), "# Surprise\n\n[[Note 1]]\n").unwrap();
        let start = Instant::now();
        let known = shell.read_with(cx, |s, _| s.notes.clone());
        let copy = start.elapsed();
        let (found, dirs, images) = vault::rescan(&root, &known);
        let start = Instant::now();
        shell.update(cx, |s, cx| s.sync(found, dirs, images, cx));
        let synced = start.elapsed();
        // A key with no effect on the text: the next frame, panel included.
        cx.simulate_keystrokes("right");
        cx.run_until_parked();
        let outside = copy + start.elapsed();
        let _ = synced;
        rows.push(format!("| {notes} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {memory} |", ms(scan), ms(rescan), ms(fingerprint), ms(startup), ms(open), ms(typing), ms(saved), ms(palette), ms(letter), ms(outside)));
        // The window and its watcher go before the next vault.
        cx.update(|_, cx| cx.windows().iter().for_each(|w| { w.update(cx, |_, window, _| window.remove_window()).ok(); }));
        cx.run_until_parked();
        fs::remove_dir_all(&root).unwrap();
    }
    println!("\n| notes | scan (ms) | rescan (ms) | fingerprint (ms) | startup (ms) | open 5000 l. (ms) | keystroke 5000 l. (ms) | save 5000 l. (ms) | palette: open (ms) | palette: one letter (ms) | note written elsewhere, main thread (ms) | memory (MB) |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
    rows.iter().for_each(|row| println!("{row}"));
}

/// Time per gesture on a big vault and a big note. Outside the regular
/// suite: `cargo test --release --locked -- --ignored --nocapture bench`.
#[gpui::test]
#[ignore]
fn bench(cx: &mut TestAppContext) {
    // Tests redo the whole layout after each reuse, to check it: not here, we measure.
    unsafe { std::env::set_var("BREF_NO_COMPARE", "1") };
    let root = std::env::temp_dir().join(format!("bref-bench-{}", std::process::id()));
    synthetic_vault(&root, 2000);
    let _config = isolated_config(&root);
    vault::save_layout("tree split 260 0");

    let start = Instant::now();
    let scanned = vault::scan(&root);
    println!("vault scan: {:?}", start.elapsed());
    let start = Instant::now();
    let again = vault::rescan(&root, &scanned.0);
    println!("new scan, nothing changed: {:?}", start.elapsed());
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
        println!("{what}: {:?} per gesture", start.elapsed() / 20);
    };
    time("keystroke", cx, &|cx| cx.simulate_input("a"));
    time("down arrow", cx, &|cx| cx.simulate_keystrokes("down"));
    time("Enter", cx, &|cx| cx.simulate_keystrokes("enter"));
    time("wheel", cx, &|cx| {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: point(px(700.), px(300.)),
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-40.))),
            ..Default::default()
        })
    });
    time("word then undo", cx, &|cx| {
        cx.simulate_input("mot ");
        cx.simulate_keystrokes("secondary-z");
    });
    // By note length: the price of a keystroke is a fixed price, plus a little per line.
    for lines in [300, 1000] {
        let body: String = (0..lines).map(|j| format!("Ligne {j} avec **gras**, `code`, [[Note {}]] et #tag{} -> fin.\n", j % 2000, j % 12)).collect();
        shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.load(format!("# Note\n\n{body}"), 100, cx)));
        cx.run_until_parked();
        time(&format!("keystroke in a {lines}-line note"), cx, &|cx| cx.simulate_input("a"));
    }
    // The same gesture on a short note: what remains is the fixed price of a keystroke.
    shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.load("# Courte\n\nun peu de texte\n".into(), 20, cx)));
    cx.run_until_parked();
    time("keystroke in a short note", cx, &|cx| cx.simulate_input("a"));
    // A big table, whose cells wrap.
    let rows: String = (0..400)
        .map(|j| format!("| Ligne {j} | texte moyen avec **gras** et `code` {j} | {}| [[Note {}]] | x | y |\n", "mot ".repeat(j % 30), j))
        .collect();
    let table = format!("# Tableau\n\n| A | B | C | D | E | F |\n| --- | --- | --- | --- | --- | --- |\n{rows}");
    shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.load(table, 120, cx)));
    cx.run_until_parked();
    time("keystroke in a 400-row table", cx, &|cx| cx.simulate_input("a"));
    time("down arrow in this table", cx, &|cx| cx.simulate_keystrokes("down"));
    time("wheel on this table", cx, &|cx| {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: point(px(700.), px(300.)),
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-40.))),
            ..Default::default()
        })
    });
    fs::remove_dir_all(&root).unwrap();
}

/// Notes that embed sections of another: opening, and a keystroke next to them (`BREF_EMBEDS` = how many).
/// `cargo test --release --locked embed_scale -- --ignored --nocapture`.
#[gpui::test]
#[ignore]
fn embed_scale(cx: &mut TestAppContext) {
    unsafe { std::env::set_var("BREF_NO_COMPARE", "1") };
    let root = std::env::temp_dir().join(format!("bref-embed-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx.run_until_parked();
    let source: String = (0..40).map(|i| format!("## Section {i}\n\nUn paragraphe assez long pour passer à la ligne dans la page, avec du *style* et du `code`.\n\n- un\n- deux\n\n| a | b |\n| - | - |\n| {i} | x |\n\n")).collect();
    fs::write(root.join("Source.md"), format!("# Source\n\n{source}")).unwrap();
    for n in std::env::var("BREF_EMBEDS").ok().map_or(vec![1usize, 5, 20, 50], |v| v.split(',').filter_map(|n| n.parse().ok()).collect()) {
        let body: String = (0..n).map(|i| format!("texte {i}\n\n![[Source#Section {}]]\n\n", i % 40)).collect();
        let file = root.join(format!("Hote{n}.md"));
        fs::write(&file, format!("# Hote\n\n{body}fin\n")).unwrap();
        shell.update(cx, |s, cx| s.rescan(&root.clone(), cx));
        cx.run_until_parked();
        let start = Instant::now();
        shell.update_in(cx, |s, window, cx| {
            s.open_note(&file, cx);
            window.focus(&s.pane().editor.focus_handle(cx));
        });
        cx.run_until_parked();
        let opened = start.elapsed();
        let at = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().len() - 2);
        shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.place_cursor(at, cx)));
        cx.run_until_parked();
        let start = Instant::now();
        for _ in 0..20 {
            cx.simulate_input("a");
            cx.run_until_parked();
        }
        let shown = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).embeds_shown());
        let found = shown.iter().filter(|(_, why, _)| why.is_none()).count();
        println!("{n:3} embeds ({found} shown): opening {opened:?} · a keystroke {:?}", start.elapsed() / 20);
    }
    fs::remove_dir_all(&root).unwrap();
}
