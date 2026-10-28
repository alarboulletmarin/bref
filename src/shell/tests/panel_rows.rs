//! The rows of the panel: an icon column that lines the names up in the tree, a name that keeps its room
//! before its folder in the recent list, measured at several panel widths and zooms.

use super::*;

fn bounds(cx: &mut gpui::VisualTestContext, name: String) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.debug_bounds(leak(name))
}

/// Folders, notes, a table, a diagram and a picture of the root: the names start at the same x, after the
/// same 16 px icon column, in every folder's depth and at every zoom.
#[gpui::test]
fn the_tree_lines_names_up_after_an_icon(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "panel-tree");
    fs::create_dir_all(root.join("Dossier")).unwrap();
    fs::write(root.join("Dossier/Dedans.md"), "# D\n").unwrap();
    for file in ["Racine.md", "table.csv", "schéma.svg", "photo.png", "X.md", "é 漢字 🎉.md"] {
        fs::write(root.join(file), "x").unwrap();
    }
    fs::write(root.join("Long ".repeat(40) + ".md"), "x").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| {
        s.nav.list_w = px(260.);
        s.show_nav(Mode::Tree, false, window, cx);
    });
    cx.simulate_resize(size(px(1000.), px(720.)));
    for zoom in ["secondary-0", "secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-="] {
        cx.simulate_keystrokes(zoom);
        let panel = bounds(cx, "nav-panel".into()).unwrap();
        let mut left = Vec::new();
        for name in ["Dossier", "Racine", "table.csv", "schéma.svg", "photo.png", "X", "é 漢字 🎉"] {
            let (icon, text) = (bounds(cx, format!("nav-icon-{name}")), bounds(cx, format!("nav-name-{name}")));
            let (icon, text) = (icon.unwrap_or_else(|| panic!("icon of {name}")), text.unwrap_or_else(|| panic!("name of {name}")));
            assert!((icon.size.width - icon.size.height).abs() < px(0.5) && icon.size.width > px(10.), "{name}: {icon:?}");
            assert!(text.left() >= icon.right() + px(3.) && text.right() <= panel.right() + px(0.5), "{name}: {icon:?} {text:?} {panel:?}");
            left.push((name, icon.left(), text.left()));
        }
        for (name, icon, text) in &left {
            assert!((*icon - left[0].1).abs() < px(0.5) && (*text - left[0].2).abs() < px(0.5), "{name} is off the column: {left:?}");
        }
        // The long name is cut inside its row.
        let long = "Long ".repeat(40);
        let text = bounds(cx, format!("nav-name-{}", long.trim_end())).or_else(|| bounds(cx, format!("nav-name-{long}")));
        if let Some(text) = text {
            assert!(text.right() <= panel.right() + px(0.5), "{text:?} {panel:?}");
        }
    }
    // Open: the folder's content is one depth deeper, its icon and name still in a column of their own.
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&root.join("Dossier/Dedans.md"), window, cx));
    cx.run_until_parked();
    let (inside, folder) = (bounds(cx, "nav-icon-Dedans".into()).unwrap(), bounds(cx, "nav-icon-Dossier".into()).unwrap());
    assert!(inside.left() > folder.left(), "{inside:?} {folder:?}");
    // The rows are 28 px high with 8 px of sheet on each side: the icon of a root folder starts after that
    // margin and the row's own 8 px (the indent is 16 px a level, whatever the zoom).
    let (panel, row) = (bounds(cx, "nav-panel".into()).unwrap(), bounds(cx, "nav-row-Dossier".into()).unwrap());
    let ui = shell.read_with(cx, |s, _| s.theme.px(1.));
    assert!((row.size.height - ui * 28.).abs() < px(0.5), "{row:?}");
    assert!(folder.left() - panel.left() >= px(16.), "{folder:?} {panel:?}");
    assert!((inside.left() - folder.left() - px(16.)).abs() < px(0.5), "{inside:?} {folder:?}");
    let _ = fs::remove_dir_all(&root);
}

/// The recent list: while the folder is shown, the name is whole (as wide as the same name alone); the folder
/// gives way first, and is hidden under its minimum.
#[gpui::test]
fn the_recent_name_keeps_its_room_before_its_folder(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "panel-recent");
    let name = "Performance - gros schémas";
    fs::create_dir_all(root.join("Performance")).unwrap();
    fs::create_dir_all(root.join("a/b/c/d")).unwrap();
    let notes = [root.join(format!("Performance/{name}.md")), root.join("Seul.md"), root.join("a/b/c/d/Profonde note avec un nom assez long pour voir.md"), root.join("a/Court.md")];
    for note in &notes {
        fs::write(note, "# N\n").unwrap();
    }
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.simulate_resize(size(px(1400.), px(720.)));
    for note in notes.iter().rev() {
        shell.update_in(cx, |s, window, cx| s.open_from_nav(note, window, cx));
    }
    shell.update_in(cx, |s, window, cx| s.show_nav(Mode::Recent, false, window, cx));
    cx.run_until_parked();
    let mut seen_folder = false;
    let mut seen_hidden = false;
    for width in [180., 260., 600.] {
        shell.update(cx, |s, cx| {
            s.nav.list_w = px(width);
            cx.notify();
        });
        let panel = bounds(cx, "nav-panel".into()).unwrap();
        for stem in [name, "Seul", "Profonde note avec un nom assez long pour voir", "Court"] {
            let (text, why) = (bounds(cx, format!("nav-name-{stem}")).unwrap(), bounds(cx, format!("nav-why-{stem}")).unwrap());
            assert!(text.right() <= panel.right() && text.right() <= why.left() + px(0.5) || why.size.width == px(0.), "{stem} at {width}: {text:?} {why:?} {panel:?}");
            assert!(why.right() <= panel.right() + px(0.5), "{stem} at {width}: {why:?} {panel:?}");
        }
        // A folder shown means the name is whole: as wide as it is when the panel is wide enough for both.
        let (text, why) = (bounds(cx, format!("nav-name-{name}")).unwrap(), bounds(cx, format!("nav-why-{name}")).unwrap());
        if why.size.width > px(0.) {
            seen_folder = true;
            let natural = text.size.width;
            shell.update(cx, |s, cx| {
                s.nav.list_w = px(900.);
                cx.notify();
            });
            let wide = bounds(cx, format!("nav-name-{name}")).unwrap();
            assert!((wide.size.width - natural).abs() < px(1.5), "the name is cut while its folder shows at {width}: {natural:?} {wide:?}");
            shell.update(cx, |s, cx| {
                s.nav.list_w = px(width);
                cx.notify();
            });
        } else {
            seen_hidden = true;
        }
    }
    assert!(seen_folder && seen_hidden, "the widths should show the folder at least once and hide it at least once");
    let _ = fs::remove_dir_all(&root);
}

/// The vault search: the field has a line of its own, the panel's full width, so its hint is whole at 260 px
/// (in French too, where it is longer); the chevron and the three settings are under it.
#[gpui::test]
fn the_vault_search_field_has_its_own_line(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "panel-search");
    fs::write(root.join("Note.md"), "# N\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.simulate_resize(size(px(1000.), px(720.)));
    cx.simulate_keystrokes("secondary-shift-f");
    for width in [180., 260., 600.] {
        shell.update(cx, |s, cx| {
            s.nav.list_w = px(width);
            cx.notify();
        });
        let panel = bounds(cx, "nav-panel".into()).unwrap();
        let (field, chevron, regex) = ["vault-query", "vault-chevron", "vault-regex"].map(|id| bounds(cx, id.into()).unwrap()).into();
        // The French hint, at 13 px, is about 150 px with its padding: whole from 260 px.
        assert!(field.size.width >= panel.size.width - px(24.), "{width}: {field:?} in {panel:?}");
        assert!(field.bottom() <= chevron.top() && field.bottom() <= regex.top(), "{width}: the settings are under the field: {field:?} {chevron:?} {regex:?}");
        if width >= 260. {
            assert!(field.size.width >= px(160.), "{width}: hint cut: {field:?}");
        }
    }
    let _ = fs::remove_dir_all(&root);
}

/// Other files than notes carry their extension next to the name: whole and on the name's line at every panel
/// width and zoom, the ellipsis in the name; notes and folders have none.
#[gpui::test]
fn a_file_keeps_its_extension_whole_when_its_name_is_cut(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "panel-ext");
    fs::create_dir_all(root.join("Dossier.v2")).unwrap();
    let long = "Très longue note de réunion ".repeat(8) + ".pdf";
    let files = ["a.csv", "photo.PNG", "archive.tar.gz", "é 漢字 🎉.pdf", "x.svg", long.as_str()];
    for file in files {
        fs::write(root.join(file), "x").unwrap();
    }
    fs::write(root.join("note.v2.md"), "x").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| s.show_nav(Mode::Tree, false, window, cx));
    cx.simulate_resize(size(px(1000.), px(720.)));
    for zoom in ["secondary-0", "secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-="] {
        cx.simulate_keystrokes(zoom);
        for width in [180., 260., 600.] {
            shell.update(cx, |s, cx| {
                s.nav.list_w = px(width);
                cx.notify();
            });
            let panel = bounds(cx, "nav-panel".into()).unwrap();
            for file in files {
                let (name, ext) = (bounds(cx, format!("nav-name-{file}")).unwrap(), bounds(cx, format!("nav-ext-{file}")).unwrap());
                let dot = file.rfind('.').unwrap();
                assert_eq!(ext.size.height, name.size.height, "{file} at {width}");
                assert!(ext.right() <= panel.right() + px(0.5) && ext.right() <= name.right() + px(0.5), "{file} at {width}: {name:?} {ext:?} {panel:?}");
                assert!(ext.left() > name.left(), "{file} at {width}: the extension comes after the name");
                // As wide as the extension alone: never cut.
                let alone = file[dot..].chars().count() as f32;
                assert!(ext.size.width >= px(alone * 3.), "{file} at {width}: {ext:?}");
            }
            assert!(bounds(cx, "nav-ext-note.v2".into()).is_none() && bounds(cx, "nav-ext-Dossier.v2".into()).is_none());
        }
    }
    let _ = fs::remove_dir_all(&root);
}
