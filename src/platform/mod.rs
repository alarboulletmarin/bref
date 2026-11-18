//! The outside world: disk, processes, network and the operating system. No gpui here.

pub mod backup;
pub mod clock;
pub mod config;
pub mod export;
pub mod git;
pub mod link;
pub mod pdf;
#[cfg(test)]
pub mod scratch;
pub mod shortcut;
pub mod signal;
pub mod terminal;
pub mod update;
pub mod vault;

use std::{ffi::OsStr, process::Command};

/// The NVIDIA Vulkan driver (package `nvidia-utils`) takes about 2 s to give up
/// when no NVIDIA card is plugged in: the loader is asked to ignore it.
#[cfg(target_os = "linux")]
pub fn skip_absent_nvidia_driver() {
    let has_nvidia = std::fs::read_dir("/sys/bus/pci/devices").is_ok_and(|devices| {
        devices.flatten().any(|d| {
            std::fs::read_to_string(d.path().join("vendor")).is_ok_and(|v| v.trim() == "0x10de")
        })
    });
    if !has_nvidia && std::env::var_os("VK_LOADER_DRIVERS_DISABLE").is_none() {
        // Safe: called at the very start of `main`, before any thread is created.
        unsafe { std::env::set_var("VK_LOADER_DRIVERS_DISABLE", "*nvidia*") };
    }
}
/// Whether the system blurs what is behind the window when asked: macOS and Windows do. On Linux, gpui asks
/// only Wayland compositors with KDE's blur protocol; X11 compositors blur by their own rules, not on request.
pub fn offers_blur() -> bool {
    cfg!(any(target_os = "macos", windows))
        || blurs_on_linux(std::env::var("WAYLAND_DISPLAY").ok().as_deref(), std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref())
}

// ponytail: told by the desktop's name (KDE); a compositor that offers the protocol under another name is missed.
// gpui 0.2.2 does not say whether it found the protocol; ask it once it does.
fn blurs_on_linux(wayland: Option<&str>, desktop: Option<&str>) -> bool {
    wayland.is_some_and(|w| !w.is_empty()) && desktop.is_some_and(|d| d.split(':').any(|d| d.eq_ignore_ascii_case("KDE")))
}

/// A process without a console window (the app has none on Windows: otherwise it flashes).
pub fn command(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_kde_on_wayland_blurs() {
        assert!(blurs_on_linux(Some("wayland-0"), Some("KDE")));
        assert!(blurs_on_linux(Some("wayland-0"), Some("ubuntu:KDE")));
        for (wayland, desktop) in [(Some("wayland-0"), Some("GNOME")), (Some(""), Some("KDE")), (None, Some("KDE")), (Some("wayland-0"), None), (Some("wayland-0"), Some(""))] {
            assert!(!blurs_on_linux(wayland, desktop), "{wayland:?} {desktop:?}");
        }
    }
}
