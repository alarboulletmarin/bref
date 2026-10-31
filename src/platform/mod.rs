//! The outside world: disk, processes, network and the operating system. No gpui here.

pub mod clock;
pub mod git;
pub mod link;
pub mod update;
pub mod vault;

use std::{ffi::OsStr, fs, process::Command};

/// The NVIDIA Vulkan driver (package `nvidia-utils`) takes about 2 s to give up
/// when no NVIDIA card is plugged in: the loader is asked to ignore it.
#[cfg(target_os = "linux")]
pub fn skip_absent_nvidia_driver() {
    let has_nvidia = fs::read_dir("/sys/bus/pci/devices").is_ok_and(|devices| {
        devices.flatten().any(|d| {
            fs::read_to_string(d.path().join("vendor")).is_ok_and(|v| v.trim() == "0x10de")
        })
    });
    if !has_nvidia && std::env::var_os("VK_LOADER_DRIVERS_DISABLE").is_none() {
        // Safe: called at the very start of `main`, before any thread is created.
        unsafe { std::env::set_var("VK_LOADER_DRIVERS_DISABLE", "*nvidia*") };
    }
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
