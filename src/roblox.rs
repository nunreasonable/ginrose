//! The macOS Roblox client, or Roblox Studio, exactly as Roblox's CDN serves
//! it. Nothing in the bundle is changed except
//! `ClientSettings/ClientAppSettings.json`, the file Roblox reads local flags
//! from.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::flavor::{self, Flavor};
use crate::paths;

const CDN: &[&str] = &[
    "https://setup.rbxcdn.com",
    "https://setup-aws.rbxcdn.com",
    "https://setup-ak.rbxcdn.com",
];
pub const USER_AGENT: &str = concat!("Ginrose/", env!("CARGO_PKG_VERSION"));

/// Where a flavor's client comes from and how its download is laid out.
pub struct Layout {
    /// The channel Roblox's version service names it by.
    pub channel: &'static str,
    /// What follows the upload in the zip's name.
    pub zip: &'static str,
    /// The bundle at the top of the zip.
    pub bundle_in_zip: &'static str,
    /// What the bundle is called once installed.
    pub app: &'static str,
    /// The bundle's main executable, in `Contents/MacOS`.
    pub executable: &'static str,
}

pub fn layout_for(flavor: Flavor) -> Layout {
    match flavor {
        Flavor::Player => Layout {
            channel: "MacPlayer",
            zip: "RobloxPlayer.zip",
            bundle_in_zip: "RobloxPlayer.app",
            app: "Roblox.app",
            executable: "RobloxPlayer",
        },
        // Studio keeps its own name: the protocol helper inside it finds the
        // app it belongs to by that.
        Flavor::Studio => Layout {
            channel: "MacStudio",
            zip: "RobloxStudioApp.zip",
            bundle_in_zip: "RobloxStudio.app",
            app: "RobloxStudio.app",
            executable: "RobloxStudio",
        },
    }
}

fn layout() -> Layout {
    layout_for(flavor::current())
}

/// Roblox's version service for a flavor, then its fallback.
pub fn version_urls_for(flavor: Flavor) -> [String; 2] {
    let channel = layout_for(flavor).channel;
    [
        format!("https://clientsettingscdn.roblox.com/v2/client-version/{channel}"),
        format!("https://clientsettings.roblox.com/v2/client-version/{channel}"),
    ]
}

/// Where a build's zip is on one of the CDN's hosts.
pub fn download_url_for(flavor: Flavor, host: &str, upload: &str) -> String {
    format!("{host}/mac/{upload}-{}", layout_for(flavor).zip)
}

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
        self.directory().join(layout().app)
    }

    pub fn is_installed(&self) -> bool {
        self.app()
            .join("Contents/MacOS")
            .join(layout().executable)
            .is_file()
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

/// For Studio's download, about 600 MB: a time limit on the whole request
/// would cut it off on an ordinary connection, so only connecting and the
/// answer starting are timed.
fn download_agent() -> ureq::Agent {
    if !flavor::is_studio() {
        return agent();
    }
    let limit = Some(std::time::Duration::from_secs(30));
    ureq::Agent::config_builder()
        .timeout_connect(limit)
        .timeout_recv_response(limit)
        .user_agent(USER_AGENT)
        .build()
        .into()
}

pub fn latest() -> Result<Build, String> {
    let agent = agent();
    let mut failures = Vec::new();
    for url in version_urls_for(flavor::current()) {
        match agent.get(&url).call() {
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
        "could not ask Roblox for the current version{} ({})",
        match flavor::current() {
            Flavor::Player => "",
            Flavor::Studio => " of Roblox Studio",
        },
        failures.join("; ")
    ))
}

pub fn installed() -> Option<Build> {
    let bytes = std::fs::read(paths::installed_file()).ok()?;
    let build: Build = serde_json::from_slice(&bytes).ok()?;
    build.is_installed().then_some(build)
}

/// A build still in the versions folder under `upload`, though maybe no
/// longer the current one: a running Studio keeps using the one it started
/// from.
pub fn installed_upload(upload: &str) -> Option<Build> {
    if let Some(build) = installed().filter(|b| b.upload == upload) {
        return Some(build);
    }
    let mut build = Build {
        version: upload.to_string(),
        upload: upload.to_string(),
    };
    let info = build.app().join("Contents/Info.plist");
    if let Some(version) = plist::Value::from_file(info).ok().and_then(|info| {
        info.as_dictionary()?
            .get("CFBundleShortVersionString")?
            .as_string()
            .map(str::to_string)
    }) {
        build.version = version;
    }
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
    let flavor = flavor::current();
    let target = dir.join(format!("{}-{}", build.upload, layout().zip));
    let partial = target.with_extension("zip.part");
    let agent = download_agent();
    let mut last = String::new();
    for host in CDN {
        let url = download_url_for(flavor, host, &build.upload);
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
                        &format!(
                            "Downloading {} {} ({} MB)",
                            flavor.client(),
                            build.version,
                            done >> 20
                        ),
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
    Err(format!("could not download {} ({last})", flavor.client()))
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
            progress(
                &format!("Unpacking {}", flavor::current().client()),
                Some(index as f64 / count as f64),
            );
        }
    }
    let layout = layout();
    let unpacked = staging.join(layout.bundle_in_zip);
    if !unpacked
        .join("Contents/MacOS")
        .join(layout.executable)
        .is_file()
    {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!("the download has no {} in it", layout.executable));
    }
    if layout.bundle_in_zip != layout.app {
        std::fs::rename(&unpacked, staging.join(layout.app)).map_err(|e| e.to_string())?;
    }
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
    progress(
        &format!("Checking for {} updates", flavor::current().client()),
        None,
    );
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

/// Throw the installed build away and download it again.
pub fn reinstall(progress: Progress) -> Result<Build, String> {
    if let Some(build) = installed() {
        std::fs::remove_dir_all(build.directory()).map_err(|e| e.to_string())?;
    }
    ensure(true, progress)
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

    #[test]
    fn each_flavor_downloads_its_own_client() {
        assert_eq!(
            version_urls_for(Flavor::Player)[0],
            "https://clientsettingscdn.roblox.com/v2/client-version/MacPlayer"
        );
        assert_eq!(
            version_urls_for(Flavor::Studio),
            [
                "https://clientsettingscdn.roblox.com/v2/client-version/MacStudio",
                "https://clientsettings.roblox.com/v2/client-version/MacStudio",
            ]
        );
        assert_eq!(
            download_url_for(Flavor::Player, CDN[0], "version-212bac6bdccd4673"),
            "https://setup.rbxcdn.com/mac/version-212bac6bdccd4673-RobloxPlayer.zip"
        );
        assert_eq!(
            download_url_for(Flavor::Studio, CDN[0], "version-a7baca1cb08a4943"),
            "https://setup.rbxcdn.com/mac/version-a7baca1cb08a4943-RobloxStudioApp.zip"
        );
        let player = layout_for(Flavor::Player);
        assert_eq!(
            (player.app, player.executable),
            ("Roblox.app", "RobloxPlayer")
        );
        let studio = layout_for(Flavor::Studio);
        assert_eq!(
            (studio.bundle_in_zip, studio.app, studio.executable),
            ("RobloxStudio.app", "RobloxStudio.app", "RobloxStudio")
        );
    }
}
