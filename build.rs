// Icon of the Windows executable: GPUI loads resource no. 1 for the window and the taskbar.
fn main() {
    // Without it, cargo reruns this script (and rebuilds the whole binary, a few minutes in release) when any
    // file of the folder changes, even one `makepkg` writes in `aur/`.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=packaging/windows");
    #[cfg(windows)]
    embed_resource::compile("packaging/windows/bref.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
