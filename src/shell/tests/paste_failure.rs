//! A pasted picture that cannot be saved says so instead of vanishing.

use super::*;
use gpui::{ClipboardItem, Image, ImageFormat};

#[gpui::test]
fn a_picture_that_cannot_be_saved_is_told(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let (root, shell, cx, _config) = start(cx, "paste-refused");
    fs::write(root.join("Note.md"), "# Note\n").unwrap();
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&root.join("Note.md"), window, cx));
    cx.run_until_parked();
    cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, vec![1, 2, 3])));
    fs::set_permissions(&root, fs::Permissions::from_mode(0o555)).unwrap();
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Paste, None, window, cx));
    cx.run_until_parked();
    let locked = fs::File::create_new(root.join("probe")).is_err();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    // Running as root nothing is refused: the picture is then saved and nothing to tell.
    if locked {
        assert!(shell.read_with(cx, |s, _| s.toasts.iter().any(|t| t.tone == Tone::Failed && t.text.contains("Image non collée"))));
        assert!(!fs::read_dir(&root).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with("image-")));
    }
    let _ = fs::remove_dir_all(root);
}
