//! The palette offers the commands of the command table: the vault and the help even when nothing is
//! typed, the others when searched for by what they do.

use super::*;

fn shows(cx: &mut gpui::VisualTestContext, action: &str) -> bool {
    cx.debug_bounds(leak(format!("command-{action}"))).is_some()
}

#[gpui::test]
fn the_palette_lists_the_table_s_commands(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-palette-commands-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (_shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
    cx.run_until_parked();
    cx.simulate_keystrokes("secondary-p");
    cx.run_until_parked();
    // Nothing typed: the two pinned ones, and no other.
    assert!(shows(cx, "app::OpenVault") && shows(cx, "app::ToggleHelp"));
    assert!(!shows(cx, "app::OpenTerminal") && !shows(cx, "app::NewDiagram"));
    for (typed, action) in [
        ("terminal", "app::OpenTerminal"),
        ("kanban", "app::NewBoard"),
        ("corbeille", "app::RestoreTrash"),
        ("santé", "app::VaultHealth"),
        ("health", "app::VaultHealth"),
        ("importer", "app::ImportDiagram"),
        ("cloner", "app::CloneVault"),
        ("capture", "app::SetUpCapture"),
        ("sauvegarder", "app::BackupVault"),
        ("git", "app::SyncVault"),
        ("propos", "app::ToggleHelp"),
        ("schéma", "app::NewDiagram"),
        ("plan", "app::Outline"),
    ] {
        cx.simulate_keystrokes("secondary-a backspace");
        cx.simulate_input(typed);
        cx.run_until_parked();
        assert!(shows(cx, action), "{typed:?} does not offer {action}");
    }
    cx.simulate_keystrokes("escape");
    fs::remove_dir_all(&root).unwrap();
}
