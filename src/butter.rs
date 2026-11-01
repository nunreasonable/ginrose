//! Rosemary builds installed by Butter, Rosemary's prefix and runtime manager,
//! so one can be chosen without looking for its binary.
//!
//! Butter keeps each runner in `<data>/butter/runners/<id>/`, described by a
//! `runner.toml` whose `executable` is relative to that folder or absolute.

use std::path::{Path, PathBuf};

use crate::paths;

pub const FLATPAK_ID: &str = "io.github.microlift.butter";

#[derive(Clone, Debug, PartialEq)]
pub struct Runner {
    /// Butter's name for it, such as "Rosemary 0.5.0 (Official Build)".
    pub name: String,
    pub executable: PathBuf,
    /// Butter's own install or its Flatpak's.
    pub flatpak: bool,
    custom: bool,
    version: Option<Vec<u64>>,
    installed: i64,
}

impl Runner {
    pub fn label(&self) -> String {
        match self.flatpak {
            true => format!("{} · Butter Flatpak", self.name),
            false => self.name.clone(),
        }
    }
}

/// Where Butter keeps runners: its own install's folder, then its Flatpak's.
fn runner_dirs() -> Vec<(PathBuf, bool)> {
    // Inside a Flatpak, XDG_DATA_HOME is the sandbox's; the host's is kept
    // in HOST_XDG_DATA_HOME.
    let host_data = ["HOST_XDG_DATA_HOME", "XDG_DATA_HOME"]
        .into_iter()
        .filter(|var| *var != "XDG_DATA_HOME" || !paths::in_flatpak())
        .find_map(|var| std::env::var_os(var).map(PathBuf::from))
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| paths::home_dir().join(".local/share"));
    let flatpak_data = paths::home_dir()
        .join(".var/app")
        .join(FLATPAK_ID)
        .join("data");
    vec![
        (host_data.join("butter/runners"), false),
        (flatpak_data.join("butter/runners"), true),
    ]
}

/// The runners whose executable exists, newest release first and the ones
/// added to Butter by hand last, as Butter lists them.
pub fn runners() -> Vec<Runner> {
    let mut list: Vec<Runner> = runner_dirs()
        .iter()
        .flat_map(|(dir, flatpak)| in_dir(dir, *flatpak))
        .filter(|r| r.executable.is_file())
        .collect();
    list.sort_by(|a, b| {
        let key = |r: &Runner| {
            (
                r.custom,
                std::cmp::Reverse(r.version.clone()),
                std::cmp::Reverse(r.installed),
                r.flatpak,
            )
        };
        key(a).cmp(&key(b)).then_with(|| a.name.cmp(&b.name))
    });
    list
}

fn in_dir(dir: &Path, flatpak: bool) -> Vec<Runner> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        // Hidden folders are installs Butter is still unpacking.
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| {
            let text = std::fs::read_to_string(e.path().join("runner.toml")).ok()?;
            parse(&e.path(), &text, flatpak)
        })
        .collect()
}

fn parse(dir: &Path, text: &str, flatpak: bool) -> Option<Runner> {
    let mut name = None;
    let mut executable = None;
    let mut kind = None;
    let mut tag = None;
    let mut installed = 0;
    for line in text.lines() {
        // Only the top-level keys Butter writes; anything after a table
        // header is someone else's.
        if line.trim_start().starts_with('[') {
            break;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "name" => name = string(value),
            "executable" => executable = string(value),
            "kind" => kind = string(value),
            "tag" => tag = string(value),
            "installed" => installed = value.parse().unwrap_or(0),
            _ => {}
        }
    }
    let executable = dir.join(executable?);
    let id = dir.file_name()?.to_string_lossy().into_owned();
    Some(Runner {
        name: name.filter(|n| !n.is_empty()).unwrap_or(id),
        executable,
        flatpak,
        custom: kind.as_deref() == Some("custom"),
        version: tag.as_deref().and_then(version),
        installed,
    })
}

/// A TOML basic ("…") or literal ('…') string on one line.
fn string(value: &str) -> Option<String> {
    if let Some(rest) = value.strip_prefix('\'') {
        return Some(rest.split_once('\'')?.0.to_string());
    }
    let rest = value.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                escape @ ('u' | 'U') => {
                    let width = if escape == 'u' { 4 } else { 8 };
                    let hex: String = chars.by_ref().take(width).collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

fn version(tag: &str) -> Option<Vec<u64>> {
    let core = tag.trim_start_matches('v').split(['-', '+']).next()?;
    core.split('.').map(|part| part.parse().ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_butter_writes() {
        let text = r#"name = "Rosemary 0.5.0 (Official Build)"
kind = "binary"
tag = "v0.5.0"
executable = "bin/rosemary"
installed = 1793000000
"#;
        let runner = parse(Path::new("/r/rosemary-v0-5-0-official"), text, false).unwrap();
        assert_eq!(runner.name, "Rosemary 0.5.0 (Official Build)");
        assert_eq!(
            runner.executable,
            PathBuf::from("/r/rosemary-v0-5-0-official/bin/rosemary")
        );
        assert_eq!(runner.version, Some(vec![0, 5, 0]));
        assert!(!runner.custom);
    }

    #[test]
    fn keeps_an_absolute_executable_and_falls_back_to_the_id() {
        let text = "kind = \"custom\"\nexecutable = '/home/me/rosemary/target/release/rosemary'\n";
        let runner = parse(Path::new("/r/my-build"), text, true).unwrap();
        assert_eq!(runner.name, "my-build");
        assert_eq!(
            runner.executable,
            PathBuf::from("/home/me/rosemary/target/release/rosemary")
        );
        assert!(runner.custom);
        assert_eq!(runner.label(), "my-build · Butter Flatpak");
    }

    #[test]
    fn unescapes_basic_strings() {
        assert_eq!(string(r#""a \"b\" \\c""#).as_deref(), Some(r#"a "b" \c"#));
        assert_eq!(string(r#""café""#).as_deref(), Some("café"));
        assert_eq!(string("unquoted"), None);
    }
}
