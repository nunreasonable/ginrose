//! Getting a Rosemary for a machine that has none: either the official
//! AppImage from microlift/rosemary's latest release, or the build Ginrose
//! publishes from Rosemary's main branch, which already has microphone input
//! and Ctrl for copy and paste.

use std::io::{Read, Write};

use serde_json::Value;

use crate::{paths, roblox};

const OFFICIAL: &str = "https://api.github.com/repos/microlift/rosemary/releases/latest";
const GINROSE: &str = "https://api.github.com/repos/nunreasonable/ginrose-flatpak/releases/latest";
/// Ginrose's build as a release asset: a plain binary, not an AppImage.
const FORK_ASSET: &str = "rosemary-ginrose-x86_64";

#[derive(Clone, Copy)]
pub enum Source {
    Official,
    Ginrose,
}

pub fn download(source: Source, progress: roblox::Progress) -> Result<std::path::PathBuf, String> {
    let (api, wanted, what) = match source {
        Source::Official => (OFFICIAL, "-x86_64.AppImage", "Rosemary"),
        Source::Ginrose => (GINROSE, FORK_ASSET, "Ginrose's Rosemary"),
    };
    let agent = roblox::agent();
    progress(&format!("Looking up the latest {what}"), None);
    let release: Value = agent
        .get(api)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("could not reach GitHub: {e}"))?
        .body_mut()
        .read_json()
        .map_err(|e| e.to_string())?;
    let tag = release["tag_name"].as_str().unwrap_or("latest").to_string();
    let url = release["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|asset| asset["browser_download_url"].as_str())
        .find(|url| url.ends_with(wanted))
        .ok_or(format!("the latest {what} release has no x86_64 build"))?
        .to_string();
    let target = paths::bundled_rosemary();
    std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
    let partial = target.with_extension("part");
    let mut response = agent
        .get(&url)
        .call()
        .map_err(|e| format!("could not download {what}: {e}"))?;
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let mut reader = response.body_mut().with_config().limit(u64::MAX).reader();
    let mut file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
    let mut buffer = vec![0u8; 1 << 16];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        progress(
            &format!("Downloading {what} {tag} ({} MB)", done >> 20),
            total.map(|t| done as f64 / t as f64),
        );
    }
    if total.is_some_and(|t| t != done) {
        let _ = std::fs::remove_file(&partial);
        return Err(format!("the {what} download stopped early"));
    }
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    std::fs::rename(&partial, &target).map_err(|e| e.to_string())?;
    Ok(target)
}
