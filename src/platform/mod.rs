//! The outside world: disk, processes, network and the operating system. No gpui here.

pub mod clock;
pub mod git;
pub mod update;
pub mod vault;

use std::{ffi::OsStr, fs, process::Command};

/// Le pilote Vulkan NVIDIA (paquet `nvidia-utils`) met environ 2 s à renoncer
/// quand aucune carte NVIDIA n'est branchée : on demande au chargeur de l'ignorer.
#[cfg(target_os = "linux")]
pub fn skip_absent_nvidia_driver() {
    let has_nvidia = fs::read_dir("/sys/bus/pci/devices").is_ok_and(|devices| {
        devices.flatten().any(|d| {
            fs::read_to_string(d.path().join("vendor")).is_ok_and(|v| v.trim() == "0x10de")
        })
    });
    if !has_nvidia && std::env::var_os("VK_LOADER_DRIVERS_DISABLE").is_none() {
        // Sûr : appelé au tout début de `main`, avant la création du moindre thread.
        unsafe { std::env::set_var("VK_LOADER_DRIVERS_DISABLE", "*nvidia*") };
    }
}
/// Un processus sans fenêtre de console (l'app n'en a pas sous Windows : sinon elle clignote).
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
