use super::*;

/// The board's header (serif title, then "N open · M done") stays in its sheet and above the columns:
/// a long title without a space, 1,000 cards, at 420, 860 and 1,600 px and at zoom 11, 16 and 32.
#[gpui::test]
fn the_board_header_stays_above_the_columns(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-board-header-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let cards: String = (0..1000).map(|i| format!("- [{}] Carte {i}\n", if i % 3 == 0 { 'x' } else { ' ' })).collect();
    let big = root.join("Mille.md");
    fs::write(&big, format!("---\nkanban: true\n---\n# {}\n\n## Tout\n\n{cards}\n## Rien\n", "Titre".repeat(20))).unwrap();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = Shell::new(Some(root.clone()), vec![big.clone()], window, cx);
        shell.client = true;
        shell
    });
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, cx| s.board_shown(cx)));
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..4 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
    };
    for (keys, zoom) in [("secondary-- ".repeat(5), 11.), ("secondary-0 ".into(), 16.), ("secondary-= ".repeat(16), 32.)] {
        cx.simulate_keystrokes(keys.trim_end());
        assert_eq!(shell.read_with(cx, |s, _| s.theme.size), zoom);
        for width in [420., 860., 1600.] {
            cx.simulate_resize(size(px(width), px(700.)));
            settle(cx);
            let what = format!("{width} px at zoom {zoom}");
            let (sheet, head, title, summary, column) = (
                cx.debug_bounds("pane-0").unwrap(),
                cx.debug_bounds("board-header").unwrap(),
                cx.debug_bounds("board-title").unwrap(),
                cx.debug_bounds("board-summary").unwrap(),
                cx.debug_bounds("column-0").unwrap(),
            );
            assert!(head.left() >= sheet.left() && head.right() <= sheet.right() + px(0.5), "{what}: header {head:?} leaves the sheet {sheet:?}");
            assert!(title.right() <= summary.left() + px(0.5), "{what}: title {title:?} runs under the summary {summary:?}");
            assert!(summary.right() <= sheet.right() + px(0.5), "{what}: summary {summary:?} leaves the sheet");
            assert!(head.bottom() <= column.top(), "{what}: header {head:?} overlaps the columns {column:?}");
            assert!(column.bottom() <= sheet.bottom(), "{what}: column {column:?} leaves the sheet");
        }
    }
    fs::remove_dir_all(&root).ok();
}
