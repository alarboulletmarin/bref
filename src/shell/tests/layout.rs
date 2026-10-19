//! The editor's incremental layout gives the same result as a full layout, edit after edit.

use super::*;

/// Thousands of random edits on varied notes: each reused layout is
/// compared (in `Editor::layout`, under test) with a full layout.
#[gpui::test]
fn layout_reuse_matches_full_layout(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-reuse-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx.run_until_parked();
    let mut seed = std::env::var("BREF_FUZZ_SEED").ok().and_then(|n| n.parse().ok()).unwrap_or(0x9E3779B97F4A7C15u64);
    let steps: usize = std::env::var("BREF_FUZZ_STEPS").ok().and_then(|n| n.parse().ok()).unwrap_or(250);
    let mut next = move |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    let table = |rows: usize| {
        let body: String = (0..rows).map(|i| format!("| {i} | produit {} | note, {i} |\n", i % 7)).collect();
        format!("| id | produit | note |\n| --- | --- | --- |\n{body}")
    };
    let docs = [
        format!("# Titre\n\nUn paragraphe.\n\n- liste un\n- liste deux\n\n> [!NOTE]\n> une note\n> suite\n\n{}\ntexte après\n\n1. un\n2. deux\n", table(6)),
        format!("# Gros\n\navant\n\n{}\naprès\n\n> citation\n\n- fin\n", table(grid::BIG + 40)),
        "# Code\n\n```rust\nfn main() {}\n```\n\ntexte\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n".to_string(),
        "texte seul\n\n---\n\n## deux\n\n- [ ] tâche\n- [x] faite\n".to_string(),
        // Formulas: their results depend on the cells around and on where the cursor is.
        "# Formules\n\navant\n\n| a | b | c |\n| --- | --- | --- |\n| 1 | 2 | =A+B |\n| 3 | 4 | =sum(above) |\n| x | =C1*2 | =B9 |\n\naprès\n".to_string(),
    ];
    let pieces = ["a", "é", " ", "| x | y |", "> [!NOTE]", "> q", "```", "- ", "1. ", "# ", "$$", "---", "![](x.png)", "|", "texte", "\n", "\n\n", "=", "=sum(above)", "=B*C", "=1/0"];
    let keys = ["enter", "backspace", "delete", "left", "right", "up", "down", "home", "end", "tab"];
    let (mut full, mut reused, mut inside) = (0, 0, 0);
    for (n, doc) in docs.into_iter().enumerate() {
        let len = doc.len();
        // A typed ``` or `$$` changes the context of everything after it: on the big table, they are avoided
        // so the test goes through the inside of the table.
        // On the big table, we stay inside its rows: enough not to cut it in two.
        let pieces: Vec<&str> = if n == 1 { vec!["a", "é", " ", "texte", "x y", "12", ","] } else { pieces.to_vec() };
        let keys: Vec<&str> = if n == 1 { vec!["left", "right", "up", "down", "end", "backspace"] } else { keys.to_vec() };
        shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.load(doc, len / 2, cx)));
        cx.run_until_parked();
        shell.update_in(cx, |s, window, cx| window.focus(&s.pane().editor.focus_handle(cx)));
        for _ in 0..steps {
            let size = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().len());
            match next(8) {
                0 => {
                    let at = next(size + 1);
                    shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.place_cursor(at, cx)));
                }
                1 | 2 => cx.simulate_keystrokes(keys[next(keys.len())]),
                _ => cx.simulate_input(pieces[next(pieces.len())]),
            }
            cx.run_until_parked();
        }
        (full, reused, inside) = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).layouts());
    }
    println!("layouts: {full} full, {reused} reused, {inside} of them in a very big table");
    assert!(inside > 20 || steps < 200, "few reuses in a very big table: {inside}");
    fs::remove_dir_all(&root).unwrap();
}
