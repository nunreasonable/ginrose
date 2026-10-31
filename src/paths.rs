use std::path::PathBuf;

use crate::flavor::{self, Flavor};

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(fallback))
}

// Each flavor keeps its own folders, named after its command, so Ginrose
// Studio never shares a prefix, a download or a setting with Ginrose.

pub fn config_dir_for(flavor: Flavor) -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join(flavor.command())
}

pub fn data_dir_for(flavor: Flavor) -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join(flavor.command())
}

pub fn cache_dir_for(flavor: Flavor) -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join(flavor.command())
}

pub fn logs_dir_for(flavor: Flavor) -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state")
        .join(flavor.command())
        .join("logs")
}

pub fn config_dir() -> PathBuf {
    config_dir_for(flavor::current())
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

pub fn data_dir() -> PathBuf {
    data_dir_for(flavor::current())
}

pub fn versions_dir() -> PathBuf {
    data_dir().join("versions")
}

pub fn installed_file() -> PathBuf {
    data_dir().join("installed.json")
}

pub fn prefix() -> PathBuf {
    data_dir().join("prefix")
}

/// Where Ginrose keeps the Rosemary it runs Roblox with, when one is installed
/// into it rather than pointed at.
pub fn bundled_rosemary() -> PathBuf {
    data_dir().join("rosemary").join("rosemary")
}

pub fn cache_dir() -> PathBuf {
    cache_dir_for(flavor::current())
}

pub fn logs_dir() -> PathBuf {
    logs_dir_for(flavor::current())
}

pub fn applications_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("applications")
}

fn guest_home() -> PathBuf {
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    prefix().join("root/Users").join(user)
}

/// The guest user's `~/Library` inside the prefix, where Roblox keeps its
/// settings (`Roblox/`) and logs (`Logs/Roblox/`).
pub fn guest_library() -> PathBuf {
    guest_home().join("Library")
}

/// The guest user's `~/Documents`, where Studio keeps its places' autosaves
/// and the user's plugins.
pub fn guest_documents() -> PathBuf {
    guest_home().join("Documents")
}

/// The folder Studio loads local plugins from, as on a Mac.
pub fn studio_plugins() -> PathBuf {
    guest_documents().join("Roblox/Plugins")
}

pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

pub fn home_dir() -> PathBuf {
    home()
}

/// Inside the Flatpak, where Rosemary comes with Ginrose and the menu entries
/// are the package's.
pub fn in_flatpak() -> bool {
    std::path::Path::new("/.flatpak-info").is_file()
}

/// The Rosemary the Flatpak ships: the only one Ginrose's Flatpak runs, and
/// the one Ginrose Studio's runs unless told otherwise.
pub const FLATPAK_ROSEMARY: &str = "/app/bin/rosemary";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_flavor_has_its_own_folders() {
        let share = xdg("XDG_DATA_HOME", ".local/share");
        assert_eq!(data_dir_for(Flavor::Player), share.join("ginrose"));
        assert_eq!(data_dir_for(Flavor::Studio), share.join("ginrose-studio"));
        assert_eq!(data_dir(), data_dir_for(Flavor::Player));
        assert!(config_dir_for(Flavor::Studio).ends_with("ginrose-studio"));
        assert!(cache_dir_for(Flavor::Studio).ends_with("ginrose-studio"));
        assert!(logs_dir_for(Flavor::Studio).ends_with("ginrose-studio/logs"));
        assert!(logs_dir_for(Flavor::Player).ends_with("ginrose/logs"));
    }

    #[test]
    fn studio_plugins_live_in_the_guest_documents() {
        assert!(studio_plugins().starts_with(prefix().join("root/Users")));
        assert!(studio_plugins().ends_with("Documents/Roblox/Plugins"));
    }
}
