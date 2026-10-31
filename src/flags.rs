//! FastFlags. Ginrose limits them to Roblox's allowlist for local
//! configuration (devforum.roblox.com/t/3966569): the client ignores every
//! other flag, so offering more would only be offering settings that do
//! nothing. Ginrose Studio writes whatever it is given, in the same file,
//! which Studio reads as it starts.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Bool,
    Int,
}

pub struct Flag {
    pub name: &'static str,
    pub kind: Kind,
    pub title: &'static str,
    pub help: &'static str,
    /// The value that trades looks for frames, offered as the suggestion.
    pub fast: Option<i64>,
}

pub const ALLOWLIST: &[Flag] = &[
    Flag {
        name: "FIntDebugForceMSAASamples",
        kind: Kind::Int,
        title: "Anti-aliasing samples",
        help: "1 turns MSAA off; 2, 4 or 8 force that much",
        fast: Some(1),
    },
    Flag {
        name: "FIntFRMMaxGrassDistance",
        kind: Kind::Int,
        title: "Grass draw distance (max)",
        help: "0 hides terrain grass, which is many draw calls",
        fast: Some(0),
    },
    Flag {
        name: "FIntFRMMinGrassDistance",
        kind: Kind::Int,
        title: "Grass draw distance (min)",
        help: "Pair with the max distance",
        fast: Some(0),
    },
    Flag {
        name: "FIntGrassMovementReducedMotionFactor",
        kind: Kind::Int,
        title: "Grass movement",
        help: "0 stops grass from swaying",
        fast: Some(0),
    },
    Flag {
        name: "DFIntTextureQualityOverride",
        kind: Kind::Int,
        title: "Texture quality",
        help: "0 to 3; needs the override switch below",
        fast: None,
    },
    Flag {
        name: "DFFlagTextureQualityOverrideEnabled",
        kind: Kind::Bool,
        title: "Override texture quality",
        help: "Use the texture quality above",
        fast: None,
    },
    Flag {
        name: "DFIntDebugFRMQualityLevelOverride",
        kind: Kind::Int,
        title: "Render quality override",
        help: "1 to 21; locks the in-game graphics slider",
        fast: None,
    },
    Flag {
        name: "DFFlagDebugPauseVoxelizer",
        kind: Kind::Bool,
        title: "Pause lighting voxelizer",
        help: "Saves CPU, but some games end up with no lighting",
        fast: None,
    },
    Flag {
        name: "FFlagDebugSkyGray",
        kind: Kind::Bool,
        title: "Gray sky",
        help: "Replaces the skybox with flat gray",
        fast: None,
    },
    Flag {
        name: "DFIntCSGLevelOfDetailSwitchingDistance",
        kind: Kind::Int,
        title: "Union detail distance",
        help: "Lower switches unions to simpler meshes sooner",
        fast: None,
    },
    Flag {
        name: "DFIntCSGLevelOfDetailSwitchingDistanceL12",
        kind: Kind::Int,
        title: "Union detail distance L1-L2",
        help: "",
        fast: None,
    },
    Flag {
        name: "DFIntCSGLevelOfDetailSwitchingDistanceL23",
        kind: Kind::Int,
        title: "Union detail distance L2-L3",
        help: "",
        fast: None,
    },
    Flag {
        name: "DFIntCSGLevelOfDetailSwitchingDistanceL34",
        kind: Kind::Int,
        title: "Union detail distance L3-L4",
        help: "",
        fast: None,
    },
];

pub fn find(name: &str) -> Option<&'static Flag> {
    ALLOWLIST.iter().find(|flag| flag.name == name)
}

/// Write the flags where the client reads them, or remove the file when there
/// are none. Flags off the allowlist are dropped rather than written.
pub fn write(app: &Path, flags: &BTreeMap<String, Value>) -> std::io::Result<()> {
    let allowed: BTreeMap<&String, &Value> = flags
        .iter()
        .filter(|(name, _)| find(name).is_some())
        .collect();
    save(app, &allowed)
}

/// Write every flag, allowlisted or not: Ginrose Studio's, where the user
/// decides what Studio gets.
pub fn write_all(app: &Path, flags: &BTreeMap<String, Value>) -> std::io::Result<()> {
    let all: BTreeMap<&String, &Value> = flags.iter().collect();
    save(app, &all)
}

fn save(app: &Path, flags: &BTreeMap<&String, &Value>) -> std::io::Result<()> {
    let path = app.join("Contents/MacOS/ClientSettings/ClientAppSettings.json");
    if flags.is_empty() {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => return Ok(()),
        }
    }
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_vec_pretty(flags)?)
}

/// A value typed by hand: `true` and `false` as booleans, whole numbers as
/// numbers, and anything else as the text it is. Roblox reads all three.
pub fn parse_value(text: &str) -> Value {
    let text = text.trim();
    if text.eq_ignore_ascii_case("true") {
        Value::from(true)
    } else if text.eq_ignore_ascii_case("false") {
        Value::from(false)
    } else if let Ok(number) = text.parse::<i64>() {
        Value::from(number)
    } else {
        Value::from(text)
    }
}

/// `Name=Value` (or `Name: Value`, or `"Name": Value` pasted from a JSON
/// list), as one flag.
pub fn parse_entry(text: &str) -> Option<(String, Value)> {
    let (name, value) = text.split_once(['=', ':'])?;
    let name = name.trim().trim_matches('"').trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    let value = value.trim().trim_end_matches(',').trim();
    let value = match value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        Some(quoted) => Value::from(quoted),
        None => parse_value(value),
    };
    Some((name.to_string(), value))
}

/// A whole flag list, as Roblox's own JSON file has it.
pub fn parse_json(text: &str) -> Result<BTreeMap<String, Value>, String> {
    let value: Value =
        serde_json::from_str(text.trim()).map_err(|e| format!("that is not JSON: {e}"))?;
    match value {
        Value::Object(map) => Ok(map.into_iter().collect()),
        _ => Err("that is not a list of flags".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_allowlisted_flags_are_written() {
        let dir = std::env::temp_dir().join(format!("ginrose-flags-{}", std::process::id()));
        let flags = BTreeMap::from([
            ("FIntDebugForceMSAASamples".to_string(), Value::from(1)),
            ("DFIntTaskSchedulerTargetFps".to_string(), Value::from(240)),
        ]);
        write(&dir, &flags).unwrap();
        let path = dir.join("Contents/MacOS/ClientSettings/ClientAppSettings.json");
        let written: BTreeMap<String, Value> =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(written.len(), 1);
        assert!(written.contains_key("FIntDebugForceMSAASamples"));
        write(&dir, &BTreeMap::new()).unwrap();
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn studio_writes_every_flag_as_given() {
        let dir = std::env::temp_dir().join(format!("ginrose-flags-all-{}", std::process::id()));
        let flags = BTreeMap::from([
            ("FIntDebugForceMSAASamples".to_string(), Value::from(1)),
            ("DFIntTaskSchedulerTargetFps".to_string(), Value::from(240)),
            ("FFlagSomethingNew".to_string(), Value::from("True")),
        ]);
        write_all(&dir, &flags).unwrap();
        let path = dir.join("Contents/MacOS/ClientSettings/ClientAppSettings.json");
        let written: BTreeMap<String, Value> =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(written, flags);
        write_all(&dir, &BTreeMap::new()).unwrap();
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn typed_flags_are_read_the_way_roblox_reads_them() {
        assert_eq!(parse_value("True"), Value::from(true));
        assert_eq!(parse_value(" 240 "), Value::from(240));
        assert_eq!(parse_value("-1"), Value::from(-1));
        assert_eq!(parse_value("1.5"), Value::from("1.5"));
        assert_eq!(
            parse_entry("DFIntTaskSchedulerTargetFps=144"),
            Some(("DFIntTaskSchedulerTargetFps".into(), Value::from(144)))
        );
        assert_eq!(
            parse_entry(r#""FFlagDebugGraphicsPreferVulkan": "True","#),
            Some(("FFlagDebugGraphicsPreferVulkan".into(), Value::from("True")))
        );
        assert_eq!(parse_entry("no value here"), None);
        assert_eq!(parse_entry("=5"), None);
        let list = parse_json(r#"{"FFlagA": true, "DFIntB": "7"}"#).unwrap();
        assert_eq!(list.get("FFlagA"), Some(&Value::from(true)));
        assert_eq!(list.get("DFIntB"), Some(&Value::from("7")));
        assert!(parse_json("[1, 2]").is_err());
    }
}
