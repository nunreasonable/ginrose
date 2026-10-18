use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(fallback))
}

pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("ginrose")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("ginrose")
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
    xdg("XDG_CACHE_HOME", ".cache").join("ginrose")
}

pub fn logs_dir() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state")
        .join("ginrose")
        .join("logs")
}

pub fn applications_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("applications")
}

/// The guest user's `~/Library` inside the prefix, where Roblox keeps its
/// settings (`Roblox/`) and logs (`Logs/Roblox/`).
pub fn guest_library() -> PathBuf {
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    prefix().join("root/Users").join(user).join("Library")
}

pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

pub fn home_dir() -> PathBuf {
    home()
}
