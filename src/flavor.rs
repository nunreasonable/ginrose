//! Which app this process is: Ginrose, the Roblox launcher, or Ginrose Studio,
//! the Roblox Studio one. Both are built from this crate and share almost all
//! of it; the binary's entry point picks the flavor once, before anything else
//! runs, and paths, IDs, downloads, menu entries and text ask it.

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    /// Ginrose, for the Roblox player.
    Player,
    /// Ginrose Studio, for Roblox Studio.
    Studio,
}

static CURRENT: OnceLock<Flavor> = OnceLock::new();

/// Set by the entry point. A second call is ignored: a process is one app.
pub fn set(flavor: Flavor) {
    let _ = CURRENT.set(flavor);
}

/// The flavor this process runs as; the player when nothing set it, which is
/// what the tests see.
pub fn current() -> Flavor {
    CURRENT.get().copied().unwrap_or(Flavor::Player)
}

pub fn is_studio() -> bool {
    current() == Flavor::Studio
}

impl Flavor {
    /// The app's name, in windows and menus.
    pub fn name(self) -> &'static str {
        match self {
            Flavor::Player => "Ginrose",
            Flavor::Studio => "Ginrose Studio",
        }
    }

    /// The command, which also names the app's folders.
    pub fn command(self) -> &'static str {
        match self {
            Flavor::Player => "ginrose",
            Flavor::Studio => "ginrose-studio",
        }
    }

    /// What the app runs, in text.
    pub fn client(self) -> &'static str {
        match self {
            Flavor::Player => "Roblox",
            Flavor::Studio => "Roblox Studio",
        }
    }

    /// The application ID, of the launcher's window and menu entry.
    pub fn app_id(self) -> &'static str {
        match self {
            Flavor::Player => "me.daeese.apps.ginrose",
            Flavor::Studio => "me.daeese.apps.ginrose.studio",
        }
    }

    /// The menu entry that starts the client and opens its links.
    pub fn client_id(self) -> &'static str {
        match self {
            Flavor::Player => "me.daeese.apps.ginrose.player",
            Flavor::Studio => "me.daeese.apps.ginrose.studio.editor",
        }
    }

    /// What Rosemary names the client's window (the bundle identifier).
    /// Taskbars and shells match a window to an icon through a desktop entry
    /// of the same name.
    pub fn window_id(self) -> &'static str {
        match self {
            Flavor::Player => "com.roblox.RobloxPlayer",
            Flavor::Studio => "com.Roblox.RobloxStudio",
        }
    }

    /// The links the client entry opens.
    pub fn schemes(self) -> &'static [&'static str] {
        match self {
            Flavor::Player => &["x-scheme-handler/roblox-player", "x-scheme-handler/roblox"],
            // `roblox-studio:` is the website's "Edit in Studio";
            // `roblox-studio-auth:` brings a browser login back to Studio.
            Flavor::Studio => &[
                "x-scheme-handler/roblox-studio",
                "x-scheme-handler/roblox-studio-auth",
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_player_keeps_its_ids() {
        let p = Flavor::Player;
        assert_eq!(p.app_id(), "me.daeese.apps.ginrose");
        assert_eq!(p.client_id(), "me.daeese.apps.ginrose.player");
        assert_eq!(p.window_id(), "com.roblox.RobloxPlayer");
        assert_eq!(p.command(), "ginrose");
        assert_eq!(current(), Flavor::Player);
    }

    #[test]
    fn studio_has_its_own_ids() {
        let s = Flavor::Studio;
        assert_eq!(s.app_id(), "me.daeese.apps.ginrose.studio");
        assert!(s.client_id().starts_with(s.app_id()));
        assert_eq!(s.window_id(), "com.Roblox.RobloxStudio");
        assert_eq!(s.command(), "ginrose-studio");
        assert_eq!(s.name(), "Ginrose Studio");
        assert_eq!(
            s.schemes(),
            [
                "x-scheme-handler/roblox-studio",
                "x-scheme-handler/roblox-studio-auth"
            ]
        );
    }
}
