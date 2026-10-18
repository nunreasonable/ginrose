<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/ginrose-wordmark-light.png">
    <img alt="Ginrose" src="assets/ginrose-wordmark.png" width="320">
  </picture>
</p>

<p align="center">A Roblox launcher for Linux, built around <a href="https://github.com/microlift/rosemary">Rosemary</a>.</p>

> [!WARNING]
> ## This is a personal project
>
> Ginrose was made for one computer (Fedora, Hyprland, an NVIDIA card) and is
> only tested there. It may well not work on yours: other distros, desktops and
> GPUs have not been tried, and the installer has not been through many hands
> yet. It is shared as it is, with no promise of support or fixes. If it breaks
> for you, an issue is welcome, but it may not get an answer soon, or at all.
>
> Rosemary itself is experimental, and so is running Roblox on it.

Ginrose runs the official macOS Roblox client on Linux through Rosemary, a
macOS compatibility layer. It downloads the client straight from Roblox, keeps
it up to date, sets it up to run well, and shows what you are playing on
Discord. The client itself is never modified: no patches, no injected code.
The only file Ginrose writes inside it is `ClientAppSettings.json`, the one
Roblox reads local settings from.

## Features

- **Updates** the macOS client from Roblox's own CDN before each launch.
- **Uncapped frame rate.** The Mac client locks to the monitor's refresh rate
  and its menu stops at 240; Ginrose lifts both.
- **FastFlags from Roblox's allowlist only**, with a one-click preset that
  turns off anti-aliasing and grass. Flags off the allowlist do nothing in the
  current client, so Ginrose does not offer them.
- **Per-game profiles**: graphics level, frame cap and flags applied when a
  link names that game.
- **Your own Discord Rich Presence**: game name, creator, icon, time played
  and a button to the game page. Games that send status through the
  BloxstrapRPC convention can set their own text and images.
- **Machine checks** before launching: a GPU handed to a virtual machine, a
  missing NVIDIA Vulkan driver, and a video wallpaper (mpvpaper) that keeps
  decoding behind the game.
- **Copy and paste with Ctrl.** The Mac client expects Command, which most
  Linux desktops keep for themselves (needs Ginrose's Rosemary build, below).
- Opens the **Play buttons on roblox.com**, and gives the Roblox window its
  icon in taskbars and docks.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/nunreasonable/ginrose/main/install.sh | sh
```

The installer puts `ginrose` in `~/.local/bin`, adds it to your app menu and
opens the setup, which finds or downloads Rosemary and Roblox. It needs GTK 4,
libadwaita and glibc 2.39 or newer (Ubuntu 24.04, Debian 13, Fedora 40 and
later). Everything stays in your home folder.

From a checkout, with Rust installed:

```sh
./install.sh --source
```

### Rosemary

The setup offers two Rosemary builds:

- **The official one** from [microlift/rosemary](https://github.com/microlift/rosemary).
  It bundles what it needs and runs on most distros.
- **Ginrose's build**, from [a fork](https://github.com/nunreasonable/rosemary/tree/mic-input)
  that adds microphone input for voice chat and lets Ctrl stand in for Command
  in copy and paste. It needs SDL 3.4 from your distro and glibc 2.39 or newer.
  To install it with Ginrose:

  ```sh
  curl -fsSL https://raw.githubusercontent.com/nunreasonable/ginrose/main/install.sh | sh -s -- --with-rosemary
  ```

## Discord

Rich Presence works out of the box while the Discord app is open: it shows the
game you're in, its icon, how long you've been playing and a link to it.

## Commands

```
ginrose                 the launcher (the setup on the first run)
ginrose setup           run the setup again
ginrose launch [link]   start Roblox, joining the game a link names
ginrose update          download the current Roblox
ginrose rosemary PATH   use a Rosemary binary you built
ginrose status          what is installed and what would get in the way
```

## Credits

- [Rosemary](https://github.com/microlift/rosemary) does the hard part:
  running macOS programs on Linux.
- [Nutmeg](https://github.com/frolleks/nutmeg) showed how a Rosemary
  bootstrapper fits together.
- The BloxstrapRPC message format comes from
  [Bloxstrap](https://github.com/bloxstraplabs/bloxstrap).

Ginrose is not affiliated with Roblox Corporation. MIT licensed; see [LICENSE](LICENSE).
