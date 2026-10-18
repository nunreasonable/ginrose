//! The macOS Roblox client, exactly as Roblox's CDN serves it. Nothing in the
//! bundle is changed except `ClientSettings/ClientAppSettings.json`, the file
//! Roblox reads local flags from.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths;

const VERSION_URLS: &[&str] = &[
    "https://clientsettingscdn.roblox.com/v2/client-version/MacPlayer",
    "https://clientsettings.roblox.com/v2/client-version/MacPlayer",
];
const CDN: &[&str] = &[
    "https://setup.rbxcdn.com",
    "https://setup-aws.rbxcdn.com",
    "https://setup-ak.rbxcdn.com",
];
const BUNDLE_IN_ZIP: &str = "RobloxPlayer.app";
pub const USER_AGENT: &str = concat!("Ginrose/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Build {
    /// `0.743.0.7431134`.
    pub version: String,
    /// `version-212bac6bdccd4673`, which names the download.
    pub upload: String,
}

impl Build {
    pub fn directory(&self) -> PathBuf {
        paths::versions_dir().join(&self.upload)
    }

    pub fn app(&self) -> PathBuf {
        self.directory().join("Roblox.app")
    }

    pub fn is_installed(&self) -> bool {
        self.app().join("Contents/MacOS/RobloxPlayer").is_file()
    }
}

#[derive(Deserialize)]
struct VersionAnswer {
    version: String,
    #[serde(rename = "clientVersionUpload")]
    upload: String,
}

pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .user_agent(USER_AGENT)
        .build()
        .into()
}

pub fn latest() -> Result<Build, String> {
    let agent = agent();
    let mut failures = Vec::new();
    for url in VERSION_URLS {
        match agent.get(*url).call() {
            Ok(mut response) => match response.body_mut().read_json::<VersionAnswer>() {
                Ok(answer) => {
                    return Ok(Build {
                        version: answer.version,
                        upload: answer.upload,
                    });
                }
                Err(e) => failures.push(format!("{url}: {e}")),
            },
            Err(e) => failures.push(format!("{url}: {e}")),
        }
    }
    Err(format!(
        "could not ask Roblox for the current version ({})",
        failures.join("; ")
    ))
}

pub fn installed() -> Option<Build> {
    let bytes = std::fs::read(paths::installed_file()).ok()?;
    let build: Build = serde_json::from_slice(&bytes).ok()?;
    build.is_installed().then_some(build)
}

fn save_installed(build: &Build) -> std::io::Result<()> {
    std::fs::create_dir_all(paths::data_dir())?;
    std::fs::write(paths::installed_file(), serde_json::to_vec_pretty(build)?)
}

/// What an install reports as it goes: a line of text and, when known, how far
/// along it is from 0 to 1.
pub type Progress<'a> = &'a dyn Fn(&str, Option<f64>);

fn download(build: &Build, progress: Progress) -> Result<PathBuf, String> {
    let dir = paths::cache_dir().join("downloads");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(format!("{}-RobloxPlayer.zip", build.upload));
    let partial = target.with_extension("zip.part");
    let agent = agent();
    let mut last = String::new();
    for host in CDN {
        let url = format!("{host}/mac/{}-RobloxPlayer.zip", build.upload);
        let mut response = match agent.get(&url).call() {
            Ok(response) => response,
            Err(e) => {
                last = format!("{url}: {e}");
                continue;
            }
        };
        let total = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        let mut reader = response.body_mut().with_config().limit(u64::MAX).reader();
        let mut file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
        let mut done = 0u64;
        let mut buffer = vec![0u8; 1 << 16];
        let result: Result<(), String> = loop {
            match reader.read(&mut buffer) {
                Ok(0) => break Ok(()),
                Ok(n) => {
                    if let Err(e) = file.write_all(&buffer[..n]) {
                        break Err(e.to_string());
                    }
                    done += n as u64;
                    let fraction = total.map(|t| done as f64 / t as f64);
                    progress(
                        &format!("Downloading Roblox {} ({} MB)", build.version, done >> 20),
                        fraction,
                    );
                }
                Err(e) => break Err(e.to_string()),
            }
        };
        match result {
            Ok(()) if total.is_none_or(|t| t == done) => {
                std::fs::rename(&partial, &target).map_err(|e| e.to_string())?;
                return Ok(target);
            }
            Ok(()) => last = format!("{url}: the download stopped early"),
            Err(e) => last = format!("{url}: {e}"),
        }
    }
    let _ = std::fs::remove_file(&partial);
    Err(format!("could not download Roblox ({last})"))
}

/// Unpack the zip into a staging folder beside the final one and move it into
/// place in one rename, so a build is either all there or not there.
fn unpack(zip_path: &Path, build: &Build, progress: Progress) -> Result<(), String> {
    let versions = paths::versions_dir();
    std::fs::create_dir_all(&versions).map_err(|e| e.to_string())?;
    let staging = versions.join(format!(".staging-{}", build.upload));
    let _ = std::fs::remove_dir_all(&staging);
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("damaged download: {e}"))?;
    let count = archive.len();
    for index in 0..count {
        let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(format!(
                "the archive has an unsafe path: {}",
                String::from_utf8_lossy(entry.name_raw())
            ));
        };
        let depth = relative.components().count();
        let out = staging.join(relative);
        let mode = entry.unix_mode().unwrap_or(0o644);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else if mode & 0o170000 == 0o120000 {
            let mut target = String::new();
            entry
                .read_to_string(&mut target)
                .map_err(|e| e.to_string())?;
            if Path::new(&target).is_absolute()
                || target.split('/').filter(|p| *p == "..").count() >= depth
            {
                return Err(format!(
                    "the archive has an unsafe link: {}",
                    String::from_utf8_lossy(entry.name_raw())
                ));
            }
            std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
            std::os::unix::fs::symlink(&target, &out).map_err(|e| e.to_string())?;
        } else {
            std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
            let mut writer = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut writer).map_err(|e| e.to_string())?;
            use std::os::unix::fs::PermissionsExt;
            let executable = mode & 0o111 != 0;
            let permissions =
                std::fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 });
            std::fs::set_permissions(&out, permissions).map_err(|e| e.to_string())?;
        }
        if index % 64 == 0 {
            progress("Unpacking Roblox", Some(index as f64 / count as f64));
        }
    }
    let unpacked = staging.join(BUNDLE_IN_ZIP);
    if !unpacked.join("Contents/MacOS/RobloxPlayer").is_file() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err("the download has no RobloxPlayer in it".into());
    }
    std::fs::rename(&unpacked, staging.join("Roblox.app")).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(build.directory());
    std::fs::rename(&staging, build.directory()).map_err(|e| e.to_string())?;
    Ok(())
}

/// Make sure the current Roblox is installed, downloading it if needed, and
/// return it. Without a network, the last installed build is used.
pub fn ensure(check: bool, progress: Progress) -> Result<Build, String> {
    let current = installed();
    if !check && let Some(build) = current {
        return Ok(build);
    }
    progress("Checking for Roblox updates", None);
    let latest = match latest() {
        Ok(latest) => latest,
        Err(e) => return current.ok_or(e),
    };
    if latest.is_installed() {
        save_installed(&latest).map_err(|e| e.to_string())?;
        return Ok(latest);
    }
    let zip = download(&latest, progress)?;
    unpack(&zip, &latest, progress)?;
    let _ = std::fs::remove_file(&zip);
    save_installed(&latest).map_err(|e| e.to_string())?;
    remove_old(&latest);
    Ok(latest)
}

/// Remove every build but `keep`, skipping any a running Roblox still uses.
fn remove_old(keep: &Build) {
    let Ok(entries) = std::fs::read_dir(paths::versions_dir()) else {
        return;
    };
    let running = crate::launch::running_builds();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == keep.upload || running.contains(&name) {
            continue;
        }
        let _ = std::fs::remove_dir_all(entry.path());
    }
}

/// The place a `roblox-player:` or `roblox:` link launches, when it names one.
pub fn place_from_uri(uri: &str) -> Option<u64> {
    let decoded = percent_decode(uri);
    let re = regex::Regex::new(r"(?i)placeId[=:](\d+)").unwrap();
    re.captures(&decoded)?.get(1)?.as_str().parse().ok()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    // Twice encoded inside `placelauncherurl`.
    let once = String::from_utf8_lossy(&out).into_owned();
    if once.contains('%') && once != text {
        percent_decode(&once)
    } else {
        once
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_place_comes_out_of_both_link_kinds() {
        let player = "roblox-player:1+launchmode:play+gameinfo:abc+placelauncherurl:https%3A%2F%2Fwww.roblox.com%2FGame%2FPlaceLauncher.ashx%3Frequest%3DRequestGame%26browserTrackerId%3D1%26placeId%3D18667984660%26isPlayTogetherGame%3Dfalse+channel:";
        assert_eq!(place_from_uri(player), Some(18667984660));
        assert_eq!(
            place_from_uri("roblox://placeId=9651695668"),
            Some(9651695668)
        );
        assert_eq!(place_from_uri("roblox-player:1+launchmode:app"), None);
    }
}
