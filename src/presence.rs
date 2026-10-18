//! Which game Roblox is in, read from the client's own log as it writes it, and
//! shown on Discord.
//!
//! Games can also set their own status by printing `[BloxstrapRPC] {json}`,
//! the convention Bloxstrap started; it is honoured when the setting allows.

use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde_json::{Value, json};

use crate::config;
use crate::discord::Client;
use crate::paths;

const HOMEPAGE: &str = "https://github.com/nunreasonable/ginrose";

#[derive(Clone, Debug, Default)]
struct Game {
    place_id: u64,
    universe_id: u64,
    name: String,
    creator: String,
    icon: String,
    since: u64,
}

/// What a game set through BloxstrapRPC, layered over the plain status.
#[derive(Clone, Debug, Default)]
struct Custom {
    details: Option<String>,
    state: Option<String>,
    start: Option<u64>,
    end: Option<u64>,
    large: Option<(String, String)>,
    small: Option<(String, String)>,
}

enum Line {
    Joining { place_id: u64 },
    Universe(u64),
    Left,
    Rpc(Value),
}

struct Patterns {
    joining: Regex,
    universe: Regex,
    left: Regex,
    rpc: Regex,
}

impl Patterns {
    fn new() -> Patterns {
        Patterns {
            joining: Regex::new(r"! Joining game '[^']*' place (\d+)").unwrap(),
            universe: Regex::new(r"universeid:(\d+)").unwrap(),
            left: Regex::new(r"leaveUGCGameInternal|Time to disconnect replication data").unwrap(),
            rpc: Regex::new(r"\[BloxstrapRPC\] (\{.*\})\s*$").unwrap(),
        }
    }

    fn parse(&self, line: &str) -> Option<Line> {
        if let Some(c) = self.joining.captures(line) {
            return Some(Line::Joining {
                place_id: c[1].parse().ok()?,
            });
        }
        if let Some(c) = self.rpc.captures(line) {
            return serde_json::from_str(&c[1]).ok().map(Line::Rpc);
        }
        if let Some(c) = self.universe.captures(line) {
            return c[1].parse().ok().map(Line::Universe);
        }
        self.left.is_match(line).then_some(Line::Left)
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn get_json(url: &str) -> Option<Value> {
    crate::roblox::agent()
        .get(url)
        .call()
        .ok()?
        .body_mut()
        .read_json::<Value>()
        .ok()
}

fn universe_of(place_id: u64) -> Option<u64> {
    get_json(&format!(
        "https://apis.roblox.com/universes/v1/places/{place_id}/universe"
    ))?["universeId"]
        .as_u64()
}

/// The game's name, creator and icon from Roblox's public APIs.
fn describe(game: &mut Game) {
    if game.universe_id == 0
        && let Some(universe) = universe_of(game.place_id)
    {
        game.universe_id = universe;
    }
    if game.universe_id == 0 {
        return;
    }
    if let Some(info) = get_json(&format!(
        "https://games.roblox.com/v1/games?universeIds={}",
        game.universe_id
    )) {
        let entry = &info["data"][0];
        game.name = entry["name"].as_str().unwrap_or_default().to_string();
        game.creator = entry["creator"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_string();
    }
    if let Some(icons) = get_json(&format!(
        "https://thumbnails.roblox.com/v1/games/icons?universeIds={}&returnPolicy=PlaceHolder&size=512x512&format=Png&isCircular=false",
        game.universe_id
    )) {
        game.icon = icons["data"][0]["imageUrl"]
            .as_str()
            .unwrap_or_default()
            .to_string();
    }
}

fn asset_image(asset_id: u64) -> Option<String> {
    let answer = get_json(&format!(
        "https://thumbnails.roblox.com/v1/assets?assetIds={asset_id}&returnPolicy=PlaceHolder&size=512x512&format=Png&isCircular=false"
    ))?;
    answer["data"][0]["imageUrl"].as_str().map(String::from)
}

/// Discord wants between 2 and 128 characters, and a button label of 32.
fn clip(text: &str, most: usize) -> String {
    let mut text: String = text.chars().take(most).collect();
    while text.chars().count() < 2 {
        text.push(' ');
    }
    text
}

fn activity(settings: &config::Discord, game: &Game, custom: &Custom) -> Value {
    let named = settings.show_game && !game.name.is_empty();
    let details = custom.details.clone().unwrap_or_else(|| match named {
        true => game.name.clone(),
        false => "In a game".into(),
    });
    let state = custom
        .state
        .clone()
        .or_else(|| (named && !game.creator.is_empty()).then(|| format!("by {}", game.creator)));
    let mut assets = serde_json::Map::new();
    let large = custom.large.clone().or_else(|| {
        (named && !game.icon.is_empty()).then(|| (game.icon.clone(), game.name.clone()))
    });
    let small = custom.small.clone().or_else(|| {
        (!settings.small_image.is_empty()).then(|| (settings.small_image.clone(), "Ginrose".into()))
    });
    // Without a game to show, the mark takes the big slot rather than leaving
    // it empty beside a small copy of itself.
    let (large, small) = match (large, small) {
        (None, Some(small)) => (Some(small), None),
        other => other,
    };
    if let Some((image, text)) = large {
        assets.insert("large_image".into(), json!(image));
        assets.insert("large_text".into(), json!(clip(&text, 128)));
    }
    if let Some((image, text)) = small {
        assets.insert("small_image".into(), json!(image));
        assets.insert("small_text".into(), json!(clip(&text, 128)));
    }
    let mut timestamps = serde_json::Map::new();
    timestamps.insert("start".into(), json!(custom.start.unwrap_or(game.since)));
    if let Some(end) = custom.end {
        timestamps.insert("end".into(), json!(end));
    }
    let mut out = json!({
        "type": 0,
        "details": clip(&details, 128),
        "timestamps": timestamps,
        "assets": assets,
    });
    if let Some(state) = state {
        out["state"] = json!(clip(&state, 128));
    }
    if named {
        // The member list says "Playing <game>" rather than the application's
        // name.
        out["status_display_type"] = json!(2);
    }
    let mut buttons = Vec::new();
    if settings.buttons && named {
        buttons.push(json!({
            "label": "See game page",
            "url": format!("https://www.roblox.com/games/{}", game.place_id),
        }));
    }
    if settings.buttons {
        buttons.push(json!({ "label": "Ginrose", "url": HOMEPAGE }));
    }
    if !buttons.is_empty() {
        out["buttons"] = json!(buttons);
    }
    out
}

/// Apply a BloxstrapRPC `SetRichPresence` message.
fn apply_rpc(custom: &mut Custom, message: &Value) {
    if message["command"] != "SetRichPresence" {
        return;
    }
    let data = &message["data"];
    let text = |key: &str| data.get(key).map(|v| v.as_str().map(String::from));
    if let Some(details) = text("details") {
        custom.details = details.filter(|s| !s.is_empty());
    }
    if let Some(state) = text("state") {
        custom.state = state.filter(|s| !s.is_empty());
    }
    if let Some(start) = data.get("timeStart") {
        custom.start = start.as_u64().filter(|t| *t != 0);
    }
    if let Some(end) = data.get("timeEnd") {
        custom.end = end.as_u64().filter(|t| *t != 0);
    }
    for (key, slot) in [
        ("largeImage", &mut custom.large),
        ("smallImage", &mut custom.small),
    ] {
        let image = &data[key];
        if image.is_null() {
            continue;
        }
        if image["reset"].as_bool() == Some(true) || image["clear"].as_bool() == Some(true) {
            *slot = None;
            continue;
        }
        if let Some(url) = image["assetId"].as_u64().and_then(asset_image) {
            let hover = image["hoverText"].as_str().unwrap_or("Roblox").to_string();
            *slot = Some((url, hover));
        }
    }
}

/// The newest client log written since `since`.
fn newest_log(since: SystemTime) -> Option<PathBuf> {
    let dir = paths::guest_library().join("Logs/Roblox");
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.contains("_Player_") && !name.contains("CrashHandler") && name.ends_with(".log")
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .filter(|(modified, _)| *modified >= since)
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

/// Follow the log until `alive` says Roblox is gone, keeping Discord up to date.
pub fn follow(settings: &config::Discord, alive: &dyn Fn() -> bool) {
    let mut client = Client::new(settings.application());
    let enabled = settings.enabled && !settings.application().is_empty();
    // `GINROSE_DEBUG=1` prints the status it would show, Discord or not.
    let debug = std::env::var_os("GINROSE_DEBUG").is_some();
    let started = SystemTime::now() - Duration::from_secs(5);
    let patterns = Patterns::new();
    let mut log: Option<(PathBuf, BufReader<std::fs::File>)> = None;
    let mut game: Option<Game> = None;
    let mut custom = Custom::default();
    let mut dirty = false;
    while alive() {
        if let Some(newest) = newest_log(started)
            && log.as_ref().is_none_or(|(path, _)| *path != newest)
            && let Ok(file) = std::fs::File::open(&newest)
        {
            log = Some((newest, BufReader::new(file)));
        }
        if let Some((_, reader)) = log.as_mut() {
            let mut line = String::new();
            loop {
                line.clear();
                let position = reader.stream_position().unwrap_or(0);
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) if !line.ends_with('\n') => {
                        // Half a line: read it again once it is finished.
                        let _ = reader.seek(SeekFrom::Start(position));
                        break;
                    }
                    Ok(_) => {}
                }
                match patterns.parse(&line) {
                    Some(Line::Joining { place_id }) => {
                        game = Some(Game {
                            place_id,
                            since: now(),
                            ..Game::default()
                        });
                        custom = Custom::default();
                        dirty = true;
                    }
                    Some(Line::Universe(universe)) => {
                        if let Some(game) = game.as_mut()
                            && game.universe_id == 0
                        {
                            game.universe_id = universe;
                        }
                    }
                    Some(Line::Left) => {
                        game = None;
                        custom = Custom::default();
                        dirty = true;
                    }
                    Some(Line::Rpc(message)) if settings.game_status && game.is_some() => {
                        if enabled {
                            apply_rpc(&mut custom, &message);
                        }
                        dirty = true;
                    }
                    _ => {}
                }
            }
        }
        if dirty && (enabled || debug) {
            if let Some(game) = game.as_mut() {
                if game.name.is_empty() && settings.show_game {
                    describe(game);
                }
                if debug {
                    eprintln!("ginrose: {}", activity(settings, game, &custom));
                }
                client.set_activity(Some(activity(settings, game, &custom)));
            } else {
                client.set_activity(None);
            }
            dirty = false;
        }
        std::thread::sleep(Duration::from_millis(750));
    }
    if enabled {
        client.set_activity(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_lines_are_recognised() {
        let p = Patterns::new();
        let joining = "2026-10-18T14:33:20.384Z,1.384774,d6c50000,6 [FLog::Output] ! Joining game 'f0e1d2c3-0000-4000-8000-000000000000' place 18667984660 at 10.0.0.1";
        assert!(matches!(
            p.parse(joining),
            Some(Line::Joining {
                place_id: 18667984660
            })
        ));
        let load = "[FLog::GameJoinLoadTime] Report game_join_loadtime: placeid:18667984660, universeid:6325043396, x";
        assert!(matches!(p.parse(load), Some(Line::Universe(6325043396))));
        assert!(matches!(
            p.parse("6 [FLog::SingleSurfaceApp] leaveUGCGameInternal"),
            Some(Line::Left)
        ));
        let rpc = r#"[FLog::Output] [BloxstrapRPC] {"command":"SetRichPresence","data":{"details":"Round 3","state":"Alive"}}"#;
        let Some(Line::Rpc(message)) = p.parse(rpc) else {
            panic!("not rpc")
        };
        let mut custom = Custom::default();
        apply_rpc(&mut custom, &message);
        assert_eq!(custom.details.as_deref(), Some("Round 3"));
        assert_eq!(custom.state.as_deref(), Some("Alive"));
    }

    #[test]
    fn the_activity_names_the_game_and_links_it() {
        let settings = config::Discord::default();
        let game = Game {
            place_id: 1,
            universe_id: 2,
            name: "Test Place".into(),
            creator: "Someone".into(),
            icon: "https://example.invalid/icon.png".into(),
            since: 100,
        };
        let out = activity(&settings, &game, &Custom::default());
        assert_eq!(out["details"], "Test Place");
        assert_eq!(out["state"], "by Someone");
        assert_eq!(out["timestamps"]["start"], 100);
        assert_eq!(out["buttons"][0]["url"], "https://www.roblox.com/games/1");
        assert_eq!(out["buttons"][1]["label"], "Ginrose");
        assert_eq!(out["status_display_type"], 2);
    }
}
