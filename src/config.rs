use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The Rosemary binary to run Roblox with. Empty means the bundled one,
    /// then `rosemary` on PATH.
    pub rosemary: String,
    /// The frame cap written into Roblox's settings before each launch. Zero
    /// leaves Roblox's own setting alone; 9999 is uncapped.
    pub frame_cap: u32,
    /// Roblox's graphics level, 1 to 10. Zero leaves Roblox's own setting.
    pub quality: u8,
    pub vsync: bool,
    /// `ROSEMARY_METAL_PRESENTATION_MODE`.
    pub presentation: String,
    pub gamemode: bool,
    pub mangohud: bool,
    /// Limit Vulkan to the NVIDIA driver when it is installed.
    pub nvidia_only: bool,
    /// `ROSEMARY_CONTROL_EDIT_KEYS`: Control-C/V/X/A/Z act as Command.
    pub control_edit_keys: bool,
    /// Keep a video wallpaper paused while Roblox runs, through mpvpaper's
    /// pause list.
    pub pause_wallpaper: bool,
    pub auto_update: bool,
    pub discord: Discord,
    /// FastFlags from Roblox's allowlist, by name.
    pub flags: BTreeMap<String, Value>,
    pub profiles: Vec<Profile>,
    pub extra_env: BTreeMap<String, String>,
    /// The first-run setup has been through to the end.
    pub setup_done: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            rosemary: String::new(),
            frame_cap: 9999,
            quality: 0,
            vsync: false,
            presentation: "mailbox".into(),
            gamemode: true,
            mangohud: false,
            nvidia_only: true,
            control_edit_keys: true,
            pause_wallpaper: true,
            auto_update: true,
            discord: Discord::default(),
            flags: BTreeMap::new(),
            profiles: Vec::new(),
            extra_env: BTreeMap::new(),
            setup_done: false,
        }
    }
}

/// Ginrose's own Discord application, which every user shares: its name is
/// what follows "Playing", and it holds the `ginrose` art asset. An application
/// ID is public; it goes out in every handshake with the Discord client.
pub const DISCORD_APP_ID: &str = "1561397757900754975";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Discord {
    pub enabled: bool,
    /// A Discord application to use instead of Ginrose's own. Empty uses
    /// [`DISCORD_APP_ID`].
    pub app_id: String,
    /// Show which game, or only that Roblox is open.
    pub show_game: bool,
    pub buttons: bool,
    /// Let games set their own status through the BloxstrapRPC messages.
    pub game_status: bool,
    /// An art asset key uploaded to the Discord application, shown small.
    pub small_image: String,
}

impl Discord {
    pub fn application(&self) -> &str {
        match self.app_id.trim() {
            "" => DISCORD_APP_ID,
            own => own,
        }
    }
}

impl Default for Discord {
    fn default() -> Self {
        Discord {
            enabled: true,
            app_id: String::new(),
            show_game: true,
            buttons: true,
            game_status: true,
            small_image: "ginrose".into(),
        }
    }
}

/// Settings that replace the defaults for one game.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub place_id: u64,
    pub name: String,
    /// Zero keeps the default.
    pub quality: u8,
    /// Zero keeps the default.
    pub frame_cap: u32,
    /// Merged over the default flags; a null value removes one.
    pub flags: BTreeMap<String, Value>,
}

impl Config {
    pub fn load() -> Config {
        std::fs::read(paths::config_file())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = paths::config_file();
        std::fs::create_dir_all(path.parent().unwrap())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)
    }

    pub fn profile(&self, place_id: u64) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.place_id == place_id)
    }

    /// The settings a launch of `place_id` runs with: the defaults, with that
    /// game's profile over them.
    pub fn effective(&self, place_id: Option<u64>) -> Effective {
        let mut effective = Effective {
            quality: self.quality,
            frame_cap: self.frame_cap,
            flags: self.flags.clone(),
            profile: None,
        };
        if let Some(profile) = place_id.and_then(|id| self.profile(id)) {
            if profile.quality != 0 {
                effective.quality = profile.quality;
            }
            if profile.frame_cap != 0 {
                effective.frame_cap = profile.frame_cap;
            }
            for (name, value) in &profile.flags {
                match value {
                    Value::Null => effective.flags.remove(name),
                    _ => effective.flags.insert(name.clone(), value.clone()),
                };
            }
            effective.profile = Some(profile.name.clone());
        }
        effective
    }

    pub fn rosemary_path(&self) -> Option<PathBuf> {
        if paths::in_flatpak() {
            let path = PathBuf::from(paths::FLATPAK_ROSEMARY);
            return path.is_file().then_some(path);
        }
        if !self.rosemary.is_empty() {
            let path = PathBuf::from(&self.rosemary);
            return path.is_file().then_some(path);
        }
        let bundled = paths::bundled_rosemary();
        if bundled.is_file() {
            return Some(bundled);
        }
        std::env::var_os("PATH").and_then(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join("rosemary"))
                .find(|candidate| candidate.is_file())
        })
    }
}

pub struct Effective {
    pub quality: u8,
    pub frame_cap: u32,
    pub flags: BTreeMap<String, Value>,
    pub profile: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_overrides_and_removes_flags() {
        let mut config = Config::default();
        config
            .flags
            .insert("FIntDebugForceMSAASamples".into(), Value::from(1));
        config
            .flags
            .insert("FIntFRMMaxGrassDistance".into(), Value::from(0));
        config.profiles.push(Profile {
            place_id: 42,
            name: "Test".into(),
            quality: 3,
            frame_cap: 0,
            flags: BTreeMap::from([
                ("FIntDebugForceMSAASamples".into(), Value::from(4)),
                ("FIntFRMMaxGrassDistance".into(), Value::Null),
            ]),
        });
        let plain = config.effective(None);
        assert_eq!(plain.quality, 0);
        assert_eq!(plain.flags.len(), 2);
        let game = config.effective(Some(42));
        assert_eq!(game.quality, 3);
        assert_eq!(game.frame_cap, 9999);
        assert_eq!(
            game.flags.get("FIntDebugForceMSAASamples"),
            Some(&Value::from(4))
        );
        assert!(!game.flags.contains_key("FIntFRMMaxGrassDistance"));
        assert_eq!(game.profile.as_deref(), Some("Test"));
    }
}
