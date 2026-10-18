//! Things about this machine that decide whether Roblox will run well, checked
//! before every launch.

use std::path::{Path, PathBuf};

use crate::paths;

pub struct Warning {
    pub title: String,
    pub detail: String,
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// A display GPU bound to `vfio-pci` has been handed to a virtual machine, and
/// Vulkan on the host cannot see it.
pub fn gpus_in_vms() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir("/sys/bus/pci/devices") else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|dev| read(&dev.join("class")).starts_with("0x03"))
        .filter(|dev| {
            std::fs::read_link(dev.join("driver"))
                .ok()
                .and_then(|d| d.file_name().map(|n| n == "vfio-pci"))
                .unwrap_or(false)
        })
        .map(|dev| dev.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

/// NVIDIA's Vulkan driver manifest, when the proprietary driver is installed.
pub fn nvidia_icd() -> Option<PathBuf> {
    [
        "/usr/share/vulkan/icd.d/nvidia_icd.x86_64.json",
        "/usr/share/vulkan/icd.d/nvidia_icd.json",
        "/etc/vulkan/icd.d/nvidia_icd.json",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}

fn process_running(name: &str) -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    entries
        .flatten()
        .any(|e| read(&e.path().join("comm")) == name)
}

/// Add the Rosemary binary's name to mpvpaper's pause list, so a video
/// wallpaper stops decoding while Roblox runs. True when the list changed, which
/// a running mpvpaper only notices once it restarts.
fn add_to_pause_list(rosemary: &Path) -> bool {
    let Some(name) = rosemary
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
    else {
        return false;
    };
    let list = paths::home_dir().join(".config/mpvpaper/pauselist");
    let text = std::fs::read_to_string(&list).unwrap_or_default();
    if text.split_whitespace().any(|entry| entry == name) {
        return false;
    }
    let mut text = text.trim_end().to_string();
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(&name);
    text.push('\n');
    std::fs::create_dir_all(list.parent().unwrap()).is_ok() && std::fs::write(&list, text).is_ok()
}

pub fn check(config: &crate::config::Config, rosemary: Option<&Path>) -> Vec<Warning> {
    let mut warnings = Vec::new();
    let in_vms = gpus_in_vms();
    if !in_vms.is_empty() {
        let fix = match which("gpu-mode") {
            true => " Run `gpu-mode host` to bring it back.",
            false => "",
        };
        warnings.push(Warning {
            title: "A GPU is attached to a virtual machine".into(),
            detail: format!(
                "{} is bound to vfio-pci, so Roblox cannot use it.{fix}",
                in_vms.join(", ")
            ),
        });
    }
    if config.nvidia_only && nvidia_icd().is_none() {
        warnings.push(Warning {
            title: "NVIDIA's Vulkan driver was not found".into(),
            detail: "\"NVIDIA only\" is on, but there is no NVIDIA Vulkan driver; Vulkan picks a GPU by itself.".into(),
        });
    }
    if config.pause_wallpaper
        && let Some(rosemary) = rosemary
        && process_running("mpvpaper")
        && add_to_pause_list(rosemary)
    {
        warnings.push(Warning {
            title: "Video wallpaper pause set up".into(),
            detail: "mpvpaper reads its pause list when it starts; restart the wallpaper once for it to pause during games.".into(),
        });
    }
    warnings
}

pub fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}
