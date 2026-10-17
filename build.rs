// Icon of the Windows executable: GPUI loads resource no. 1 for the window and the taskbar.
fn main() {
    #[cfg(windows)]
    embed_resource::compile("packaging/windows/bref.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
