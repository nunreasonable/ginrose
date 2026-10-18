//! Roblox's own settings file, `GlobalBasicSettings_<n>.xml`. Roblox writes
//! it back when it closes, and its menu only reaches 240 frames, so the cap and
//! the graphics level are put back before every launch.

use regex::Regex;

use crate::paths;

fn settings_files() -> Vec<std::path::PathBuf> {
    let dir = paths::guest_library().join("Roblox");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("GlobalBasicSettings_") && n.ends_with(".xml"))
        })
        .collect()
}

/// Set an element's value in the settings text, if the element is there.
fn set(text: &str, tag: &str, name: &str, value: &str) -> String {
    let pattern = format!(r#"(<{tag} name="{}">)[^<]*(</{tag}>)"#, regex::escape(name));
    let re = Regex::new(&pattern).expect("a valid pattern");
    re.replace(text, format!("${{1}}{value}${{2}}"))
        .into_owned()
}

pub fn apply_text(text: &str, frame_cap: u32, quality: u8) -> String {
    let mut text = text.to_string();
    if frame_cap != 0 {
        text = set(&text, "int", "FramerateCap", &frame_cap.to_string());
    }
    if quality != 0 {
        text = set(&text, "token", "SavedQualityLevel", &quality.to_string());
        // The in-game level runs 1 to 21; the saved one 1 to 10.
        let level = (u32::from(quality) * 21).div_ceil(10).min(21);
        text = set(&text, "int", "GraphicsQualityLevel", &level.to_string());
    }
    text
}

/// Put the frame cap and graphics level into every settings file. A first run
/// has none yet; Roblox makes it, and the next launch fills it in.
pub fn apply(frame_cap: u32, quality: u8) {
    for path in settings_files() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let updated = apply_text(&text, frame_cap, quality);
        if updated != text {
            let _ = std::fs::write(&path, updated);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<Item class="UserGameSettings">
			<int name="FramerateCap">240</int>
			<int name="GraphicsQualityLevel">0</int>
			<token name="SavedQualityLevel">0</token>
</Item>"#;

    #[test]
    fn the_cap_and_quality_are_set_and_nothing_else_moves() {
        let out = apply_text(SAMPLE, 9999, 5);
        assert!(out.contains(r#"<int name="FramerateCap">9999</int>"#));
        assert!(out.contains(r#"<token name="SavedQualityLevel">5</token>"#));
        assert!(out.contains(r#"<int name="GraphicsQualityLevel">11</int>"#));
        assert_eq!(out.lines().count(), SAMPLE.lines().count());
    }

    #[test]
    fn zero_leaves_roblox_alone() {
        assert_eq!(apply_text(SAMPLE, 0, 0), SAMPLE);
    }
}
