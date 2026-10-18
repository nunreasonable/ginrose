//! Menu entries, and opening `roblox-player:` and `roblox:` links from the
//! browser.

use std::process::Command;

use crate::paths;

const APP_ID: &str = "io.github.nunreasonable.Ginrose";
const PLAYER_ID: &str = "io.github.nunreasonable.Ginrose.Player";
/// What Rosemary names the Roblox window (the bundle identifier). Taskbars and
/// shells match a window to an icon through a desktop entry of the same name.
const WINDOW_ID: &str = "com.roblox.RobloxPlayer";
const SCHEMES: &[&str] = &["x-scheme-handler/roblox-player", "x-scheme-handler/roblox"];

/// The mark, at the sizes icon themes look for.
const ICONS: &[(u32, &[u8])] = &[
    (16, include_bytes!("../assets/icons/16.png")),
    (24, include_bytes!("../assets/icons/24.png")),
    (32, include_bytes!("../assets/icons/32.png")),
    (48, include_bytes!("../assets/icons/48.png")),
    (64, include_bytes!("../assets/icons/64.png")),
    (128, include_bytes!("../assets/icons/128.png")),
    (256, include_bytes!("../assets/icons/256.png")),
    (512, include_bytes!("../assets/icons/512.png")),
];

fn icon_theme() -> std::path::PathBuf {
    paths::applications_dir()
        .parent()
        .unwrap()
        .join("icons/hicolor")
}

fn install_icons() -> Result<(), String> {
    let theme = icon_theme();
    for (size, bytes) in ICONS {
        let dir = theme.join(format!("{size}x{size}/apps"));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(format!("{APP_ID}.png")), bytes).map_err(|e| e.to_string())?;
    }
    let _ = Command::new("gtk-update-icon-cache")
        .args(["-f", "-t"])
        .arg(&theme)
        .status();
    Ok(())
}

fn quote(arg: &str) -> String {
    if arg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-+".contains(c))
    {
        arg.to_string()
    } else {
        format!(
            "\"{}\"",
            arg.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('`', "\\`")
                .replace('$', "\\$")
        )
    }
}

/// Write both entries. With `default`, also make Ginrose the program that opens
/// Roblox links, taking them over from whatever opened them before.
pub fn install(default: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = quote(&exe.to_string_lossy());
    install_icons()?;
    let dir = paths::applications_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let settings = format!(
        "[Desktop Entry]\nType=Application\nName=Ginrose\nComment=Roblox launcher and settings\nExec={exe}\nIcon={APP_ID}\nCategories=Game;\nStartupWMClass={APP_ID}\n"
    );
    let player = format!(
        "[Desktop Entry]\nType=Application\nName=Roblox\nGenericName=Roblox (Ginrose)\nComment=Play Roblox through Ginrose\nExec={exe} launch %u\nIcon={APP_ID}\nCategories=Game;\nMimeType={};\nStartupNotify=false\nStartupWMClass={WINDOW_ID}\n",
        SCHEMES.join(";")
    );
    std::fs::write(dir.join(format!("{APP_ID}.desktop")), settings).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{PLAYER_ID}.desktop")), player).map_err(|e| e.to_string())?;
    let window = format!(
        "[Desktop Entry]\nType=Application\nName=Roblox\nExec={exe} launch\nIcon={APP_ID}\nStartupWMClass={WINDOW_ID}\nNoDisplay=true\n"
    );
    std::fs::write(dir.join(format!("{WINDOW_ID}.desktop")), window).map_err(|e| e.to_string())?;
    let _ = Command::new("update-desktop-database").arg(&dir).status();
    if default {
        let status = Command::new("xdg-mime")
            .arg("default")
            .arg(format!("{PLAYER_ID}.desktop"))
            .args(SCHEMES)
            .status()
            .map_err(|e| format!("could not run xdg-mime: {e}"))?;
        if !status.success() {
            return Err("xdg-mime could not set the default".into());
        }
    }
    Ok(())
}

pub fn uninstall() {
    let dir = paths::applications_dir();
    let _ = std::fs::remove_file(dir.join(format!("{APP_ID}.desktop")));
    let _ = std::fs::remove_file(dir.join(format!("{PLAYER_ID}.desktop")));
    let _ = std::fs::remove_file(dir.join(format!("{WINDOW_ID}.desktop")));
    let _ = Command::new("update-desktop-database").arg(&dir).status();
    for (size, _) in ICONS {
        let _ = std::fs::remove_file(icon_theme().join(format!("{size}x{size}/apps/{APP_ID}.png")));
    }
}

/// Whether Ginrose opens Roblox links now.
pub fn is_default() -> bool {
    Command::new("xdg-mime")
        .args(["query", "default", SCHEMES[0]])
        .output()
        .is_ok_and(|out| {
            String::from_utf8_lossy(&out.stdout).trim() == format!("{PLAYER_ID}.desktop")
        })
}

pub fn app_id() -> &'static str {
    APP_ID
}
