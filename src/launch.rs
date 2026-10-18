//! Starting Roblox through Rosemary, and stopping one already running.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use crate::config::Config;
use crate::{flags, machine, paths, roblox, settings};

/// Process ids of the Rosemary processes running a build Ginrose installed,
/// with the build each runs.
fn ours() -> Vec<(i32, String)> {
    let versions = paths::versions_dir().to_string_lossy().into_owned();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let pid: i32 = e.file_name().to_str()?.parse().ok()?;
            let cmdline = std::fs::read(e.path().join("cmdline")).ok()?;
            let args: Vec<String> = cmdline
                .split(|b| *b == 0)
                .map(|a| String::from_utf8_lossy(a).into_owned())
                .collect();
            let build = args.iter().find_map(|a| {
                let rest = a.strip_prefix(&versions)?.trim_start_matches('/');
                Some(rest.split('/').next()?.to_string())
            })?;
            Some((pid, build))
        })
        .filter(|(pid, _)| *pid as u32 != std::process::id())
        .collect()
}

pub fn running_builds() -> BTreeSet<String> {
    ours().into_iter().map(|(_, build)| build).collect()
}

/// Ask a running Roblox to quit, then make it.
pub fn stop_running() {
    let pids: Vec<i32> = ours().into_iter().map(|(pid, _)| pid).collect();
    if pids.is_empty() {
        return;
    }
    for pid in &pids {
        unsafe_kill(*pid, 15);
    }
    for _ in 0..50 {
        if ours().is_empty() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    for (pid, _) in ours() {
        unsafe_kill(pid, 9);
    }
}

fn unsafe_kill(pid: i32, signal: i32) {
    let _ = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(pid.to_string())
        .stderr(Stdio::null())
        .status();
}

/// What a launch needs ready, prepared in order: Rosemary, Roblox, the prefix,
/// then the settings for the game being joined.
pub struct Prepared {
    pub rosemary: PathBuf,
    pub build: roblox::Build,
    pub warnings: Vec<machine::Warning>,
    pub profile: Option<String>,
}

pub fn prepare(
    config: &Config,
    uri: Option<&str>,
    progress: roblox::Progress,
) -> Result<Prepared, String> {
    let rosemary = config.rosemary_path().ok_or(
        "Rosemary was not found. Set its path in Settings, or install one with `ginrose rosemary <path>`.",
    )?;
    let warnings = machine::check(config, Some(&rosemary));
    let build = roblox::ensure(config.auto_update, progress)?;
    if !paths::prefix().join("root").is_dir() {
        progress("Setting up the Rosemary prefix", None);
        let status = Command::new(&rosemary)
            .args(["prefix", "create"])
            .arg(paths::prefix())
            .env("ROSEMARY_PREFIX_SETUP", "download")
            .stdin(Stdio::null())
            .status()
            .map_err(|e| format!("could not run Rosemary: {e}"))?;
        if !status.success() {
            return Err(format!("`rosemary prefix create` failed ({status})"));
        }
    }
    let effective = config.effective(uri.and_then(roblox::place_from_uri));
    settings::apply(effective.frame_cap, effective.quality);
    flags::write(&build.app(), &effective.flags)
        .map_err(|e| format!("could not write the FastFlags: {e}"))?;
    Ok(Prepared {
        rosemary,
        build,
        warnings,
        profile: effective.profile,
    })
}

fn environment(config: &Config, command: &mut Command) {
    command.env("ROSEMARY_PREFIX", paths::prefix());
    command.env("ROSEMARY_PREFIX_SETUP", "download");
    if !config.vsync {
        command.env("ROSEMARY_SWAP_INTERVAL", "0");
        // The Mac client paces itself to the refresh rate it is told.
        command.env("ROSEMARY_SCREEN_REFRESH_RATE", "9999");
    }
    if !config.presentation.is_empty() {
        command.env("ROSEMARY_METAL_PRESENTATION_MODE", &config.presentation);
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        command.env("SDL_VIDEO_DRIVER", "wayland");
    }
    if config.nvidia_only
        && machine::gpus_in_vms().is_empty()
        && let Some(icd) = machine::nvidia_icd()
    {
        command.env("VK_DRIVER_FILES", icd);
        command.env("__GL_SHADER_DISK_CACHE_SIZE", "10737418240");
        command.env("__GL_SHADER_DISK_CACHE_SKIP_CLEANUP", "1");
    }
    if config.control_edit_keys {
        command.env("ROSEMARY_CONTROL_EDIT_KEYS", "1");
    }
    command.envs(&config.extra_env);
}

pub struct Launched {
    pub child: Child,
    /// Rosemary's and Roblox's output, which says when the window is up.
    pub log: PathBuf,
}

pub fn spawn(config: &Config, prepared: &Prepared, uri: Option<&str>) -> Result<Launched, String> {
    stop_running();
    let mut argv: Vec<PathBuf> = Vec::new();
    if config.gamemode && machine::which("gamemoderun") {
        argv.push("gamemoderun".into());
    }
    if config.mangohud && machine::which("mangohud") {
        argv.push("mangohud".into());
    }
    argv.push(prepared.rosemary.clone());
    let mut command = Command::new(&argv[0]);
    command.args(&argv[1..]);
    command.arg("run").arg(prepared.build.app());
    if let Some(uri) = uri.filter(|u| !u.is_empty()) {
        command.arg("-protocolString").arg(uri);
    }
    environment(config, &mut command);
    let (log, path) = open_log()?;
    command
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log);
    let child = command
        .spawn()
        .map_err(|e| format!("could not start Roblox: {e}"))?;
    Ok(Launched { child, log: path })
}

/// Wait until Roblox has drawn its first frame, which is when its window is
/// on screen, or until `limit`. False when the limit came first.
pub fn wait_for_window(log: &Path, limit: Duration) -> bool {
    use std::io::Read;
    let deadline = std::time::Instant::now() + limit;
    let mut seen = String::new();
    let mut offset = 0usize;
    while std::time::Instant::now() < deadline {
        if let Ok(mut file) = std::fs::File::open(log) {
            let mut bytes = Vec::new();
            if file.read_to_end(&mut bytes).is_ok() && bytes.len() > offset {
                // Keep a little of the last read, in case the line was split.
                let start = offset.saturating_sub(64);
                seen = String::from_utf8_lossy(&bytes[start..]).into_owned();
                offset = bytes.len();
                if seen.contains("resizing main framebuffer") {
                    return true;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = seen;
    false
}

fn open_log() -> Result<(std::fs::File, PathBuf), String> {
    let dir = paths::logs_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    prune_logs(&dir);
    let path = dir.join(format!("roblox-{stamp}.log"));
    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    Ok((file, path))
}

/// Keep the newest twenty logs.
fn prune_logs(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut logs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    logs.sort();
    while logs.len() > 20 {
        let _ = std::fs::remove_file(logs.remove(0));
    }
}
