//! Menu entries, and opening `roblox-player:` and `roblox:` links (Ginrose) or
//! `roblox-studio:` and `roblox-studio-auth:` links (Ginrose Studio) from the
//! browser.

use std::process::Command;

use crate::flavor::{self, Flavor};
use crate::paths;

type Icons = &'static [(u32, &'static [u8])];

/// The mark, at the sizes icon themes look for.
const ICONS: Icons = &[
    (16, include_bytes!("../assets/icons/16.png")),
    (24, include_bytes!("../assets/icons/24.png")),
    (32, include_bytes!("../assets/icons/32.png")),
    (48, include_bytes!("../assets/icons/48.png")),
    (64, include_bytes!("../assets/icons/64.png")),
    (128, include_bytes!("../assets/icons/128.png")),
    (256, include_bytes!("../assets/icons/256.png")),
    (512, include_bytes!("../assets/icons/512.png")),
];

const STUDIO_ICONS: Icons = &[
    (16, include_bytes!("../assets/studio-icons/16.png")),
    (24, include_bytes!("../assets/studio-icons/24.png")),
    (32, include_bytes!("../assets/studio-icons/32.png")),
    (48, include_bytes!("../assets/studio-icons/48.png")),
    (64, include_bytes!("../assets/studio-icons/64.png")),
    (128, include_bytes!("../assets/studio-icons/128.png")),
    (256, include_bytes!("../assets/studio-icons/256.png")),
    (512, include_bytes!("../assets/studio-icons/512.png")),
];

fn icons() -> Icons {
    match flavor::current() {
        Flavor::Player => ICONS,
        Flavor::Studio => STUDIO_ICONS,
    }
}

fn icon_theme() -> std::path::PathBuf {
    paths::applications_dir()
        .parent()
        .unwrap()
        .join("icons/hicolor")
}

fn install_icons() -> Result<(), String> {
    let theme = icon_theme();
    let id = app_id();
    for (size, bytes) in icons() {
        let dir = theme.join(format!("{size}x{size}/apps"));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(format!("{id}.png")), bytes).map_err(|e| e.to_string())?;
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

/// What makes the Flatpak's client entry open the links, to run on the host:
/// a sandboxed program cannot change that itself.
pub fn host_default_command() -> String {
    let flavor = flavor::current();
    format!(
        "xdg-mime default {}.desktop {}",
        flavor.client_id(),
        flavor.schemes().join(" ")
    )
}

/// The three entries a native install writes, by file name: the launcher, the
/// client with the links it opens, and a hidden one named after the client's
/// window, which gives that window its icon.
pub fn entries(flavor: Flavor, exe: &str) -> [(String, String); 3] {
    let (app, client, window) = (flavor.app_id(), flavor.client_id(), flavor.window_id());
    let schemes = flavor.schemes().join(";");
    match flavor {
        Flavor::Player => [
            (
                format!("{app}.desktop"),
                format!(
                    "[Desktop Entry]\nType=Application\nName=Ginrose\nComment=Roblox launcher and settings\nExec={exe}\nIcon={app}\nCategories=Game;\nStartupWMClass={app}\n"
                ),
            ),
            (
                format!("{client}.desktop"),
                format!(
                    "[Desktop Entry]\nType=Application\nName=Roblox\nGenericName=Roblox (Ginrose)\nComment=Play Roblox through Ginrose\nExec={exe} launch %u\nIcon={app}\nCategories=Game;\nMimeType={schemes};\nStartupNotify=false\nStartupWMClass={window}\n"
                ),
            ),
            (
                format!("{window}.desktop"),
                format!(
                    "[Desktop Entry]\nType=Application\nName=Roblox\nExec={exe} launch\nIcon={app}\nStartupWMClass={window}\nNoDisplay=true\n"
                ),
            ),
        ],
        Flavor::Studio => [
            (
                format!("{app}.desktop"),
                format!(
                    "[Desktop Entry]\nType=Application\nName=Ginrose Studio\nComment=Roblox Studio launcher and settings\nExec={exe}\nIcon={app}\nCategories=Development;\nStartupWMClass={app}\n"
                ),
            ),
            (
                format!("{client}.desktop"),
                format!(
                    "[Desktop Entry]\nType=Application\nName=Roblox Studio\nGenericName=Roblox Studio (Ginrose Studio)\nComment=Build Roblox experiences through Ginrose Studio\nExec={exe} launch %u\nIcon={app}\nCategories=Development;\nMimeType={schemes};\nStartupNotify=false\nStartupWMClass={window}\n"
                ),
            ),
            (
                format!("{window}.desktop"),
                format!(
                    "[Desktop Entry]\nType=Application\nName=Roblox Studio\nExec={exe} launch\nIcon={app}\nStartupWMClass={window}\nNoDisplay=true\n"
                ),
            ),
        ],
    }
}

/// Whether a distribution package installed the entries, in one of the
/// system's data folders (`packaging/desktop`). Writing them again for this
/// user would list everything twice in the menu.
pub fn packaged() -> bool {
    let dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let name = format!("{}.desktop", flavor::current().client_id());
    std::env::split_paths(&dirs).any(|dir| dir.join("applications").join(&name).is_file())
}

/// Write the entries. With `default`, also make this app the program that
/// opens its links, taking them over from whatever opened them before. The
/// Flatpak's entries come with the package, and so do a distribution
/// package's.
pub fn install(default: bool) -> Result<(), String> {
    if paths::in_flatpak() {
        return Ok(());
    }
    let flavor = flavor::current();
    if !packaged() {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = quote(&exe.to_string_lossy());
        install_icons()?;
        let dir = paths::applications_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        for (name, text) in entries(flavor, &exe) {
            std::fs::write(dir.join(name), text).map_err(|e| e.to_string())?;
        }
        let _ = Command::new("update-desktop-database").arg(&dir).status();
    }
    if default {
        let status = Command::new("xdg-mime")
            .arg("default")
            .arg(format!("{}.desktop", flavor.client_id()))
            .args(flavor.schemes())
            .status()
            .map_err(|e| format!("could not run xdg-mime: {e}"))?;
        if !status.success() {
            return Err("xdg-mime could not set the default".into());
        }
    }
    Ok(())
}

pub fn uninstall() {
    if paths::in_flatpak() {
        return;
    }
    let dir = paths::applications_dir();
    for (name, _) in entries(flavor::current(), "") {
        let _ = std::fs::remove_file(dir.join(name));
    }
    let _ = Command::new("update-desktop-database").arg(&dir).status();
    for (size, _) in icons() {
        let _ =
            std::fs::remove_file(icon_theme().join(format!("{size}x{size}/apps/{}.png", app_id())));
    }
}

/// Whether this app opens its links now.
pub fn is_default() -> bool {
    let flavor = flavor::current();
    Command::new("xdg-mime")
        .args(["query", "default", flavor.schemes()[0]])
        .output()
        .is_ok_and(|out| {
            String::from_utf8_lossy(&out.stdout).trim() == format!("{}.desktop", flavor.client_id())
        })
}

pub fn app_id() -> &'static str {
    flavor::current().app_id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_player_entries_are_unchanged() {
        let [settings, player, window] = entries(Flavor::Player, "/bin/ginrose");
        assert_eq!(settings.0, "me.daeese.apps.ginrose.desktop");
        assert_eq!(
            settings.1,
            "[Desktop Entry]\nType=Application\nName=Ginrose\nComment=Roblox launcher and settings\nExec=/bin/ginrose\nIcon=me.daeese.apps.ginrose\nCategories=Game;\nStartupWMClass=me.daeese.apps.ginrose\n"
        );
        assert_eq!(player.0, "me.daeese.apps.ginrose.player.desktop");
        assert_eq!(
            player.1,
            "[Desktop Entry]\nType=Application\nName=Roblox\nGenericName=Roblox (Ginrose)\nComment=Play Roblox through Ginrose\nExec=/bin/ginrose launch %u\nIcon=me.daeese.apps.ginrose\nCategories=Game;\nMimeType=x-scheme-handler/roblox-player;x-scheme-handler/roblox;\nStartupNotify=false\nStartupWMClass=com.roblox.RobloxPlayer\n"
        );
        assert_eq!(window.0, "com.roblox.RobloxPlayer.desktop");
        assert_eq!(
            window.1,
            "[Desktop Entry]\nType=Application\nName=Roblox\nExec=/bin/ginrose launch\nIcon=me.daeese.apps.ginrose\nStartupWMClass=com.roblox.RobloxPlayer\nNoDisplay=true\n"
        );
    }

    #[test]
    fn the_packaged_entries_are_the_ones_ginrose_writes() {
        for (flavor, command) in [
            (Flavor::Player, "ginrose"),
            (Flavor::Studio, "ginrose-studio"),
        ] {
            for (name, text) in entries(flavor, command) {
                let path = format!("{}/packaging/desktop/{name}", env!("CARGO_MANIFEST_DIR"));
                let packaged = std::fs::read_to_string(&path).unwrap_or_default();
                assert_eq!(packaged, text, "{path} is out of date");
            }
        }
    }

    #[test]
    fn studio_entries_open_its_links_and_name_its_window() {
        let [settings, client, window] = entries(Flavor::Studio, "/bin/ginrose-studio");
        assert_eq!(settings.0, "me.daeese.apps.ginrose.studio.desktop");
        assert!(settings.1.contains("\nName=Ginrose Studio\n"));
        assert!(
            settings
                .1
                .contains("\nIcon=me.daeese.apps.ginrose.studio\n")
        );
        assert_eq!(client.0, "me.daeese.apps.ginrose.studio.editor.desktop");
        assert!(client.1.contains("\nExec=/bin/ginrose-studio launch %u\n"));
        assert!(client.1.contains(
            "\nMimeType=x-scheme-handler/roblox-studio;x-scheme-handler/roblox-studio-auth;\n"
        ));
        assert!(
            client
                .1
                .contains("\nStartupWMClass=com.Roblox.RobloxStudio\n")
        );
        assert_eq!(window.0, "com.Roblox.RobloxStudio.desktop");
        assert!(window.1.contains("NoDisplay=true"));
    }
}
