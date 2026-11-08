//! The region of the Roblox server the player is on, shown over a corner of the
//! game. Ginrose draws it into a small picture that Ginrose's Rosemary copies
//! over every frame (`ROSEMARY_OVERLAY_IMAGE`). Opt-in: the region comes from
//! asking ipinfo.io about the server's address.

use std::path::PathBuf;
use std::process::Command;

use gtk::cairo;
use serde_json::Value;

use crate::config::Config;
use crate::paths;

/// The corners Rosemary accepts, and what the settings call them.
pub const CORNERS: [(&str, &str); 4] = [
    ("top-left", "Top left"),
    ("top-right", "Top right"),
    ("bottom-left", "Bottom left"),
    ("bottom-right", "Bottom right"),
];

pub const DEFAULT_CORNER: &str = "top-right";

const FONT_SIZE: f64 = 15.0;
const PADDING: f64 = 8.0;
const BACKGROUND: (f64, f64, f64) = (0.08, 0.08, 0.1);
const TEXT: (f64, f64, f64) = (0.95, 0.95, 0.97);

/// Where `name` is in [`CORNERS`], or the default's place.
pub fn corner_index(name: &str) -> u32 {
    CORNERS
        .iter()
        .position(|(corner, _)| *corner == name)
        .or_else(|| {
            CORNERS
                .iter()
                .position(|(corner, _)| *corner == DEFAULT_CORNER)
        })
        .unwrap_or_default() as u32
}

/// The corner at `index` in [`CORNERS`], or the default.
pub fn corner_name(index: u32) -> &'static str {
    CORNERS
        .get(index as usize)
        .map_or(DEFAULT_CORNER, |(name, _)| name)
}

fn badge_path() -> PathBuf {
    paths::runtime_dir().join("ginrose-server-region.ppm")
}

/// Ask Rosemary to show the picture, starting with none.
pub fn environment(config: &Config, command: &mut Command) {
    if !config.region_badge {
        return;
    }
    let path = badge_path();
    let _ = std::fs::write(&path, b"");
    command.env("ROSEMARY_OVERLAY_IMAGE", path);
    command.env(
        "ROSEMARY_OVERLAY_CORNER",
        corner_name(corner_index(&config.region_corner)),
    );
}

/// "City, Country" for a server's public address.
pub fn locate(address: &str) -> Option<String> {
    let info: Value = crate::roblox::agent()
        .get(&format!("https://ipinfo.io/{address}/json"))
        .call()
        .ok()?
        .body_mut()
        .read_json()
        .ok()?;
    let part = |key: &str| info[key].as_str().filter(|value| !value.is_empty());
    match (part("city"), part("country")) {
        (Some(city), Some(country)) => Some(format!("{city}, {country}")),
        (None, Some(country)) => Some(country.to_string()),
        (Some(city), None) => Some(city.to_string()),
        (None, None) => None,
    }
}

/// Show `text`, or nothing. The file is replaced whole, so Rosemary never
/// reads half a picture.
pub fn show(text: Option<&str>) {
    let bytes = text.and_then(draw).unwrap_or_default();
    let path = badge_path();
    let partial = path.with_extension("part");
    if std::fs::write(&partial, bytes).is_ok() {
        let _ = std::fs::rename(&partial, &path);
    }
}

/// `text` on a dark plate, as a binary PPM.
fn draw(text: &str) -> Option<Vec<u8>> {
    let font = |context: &cairo::Context| {
        context.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
        context.set_font_size(FONT_SIZE);
    };
    let measure = cairo::ImageSurface::create(cairo::Format::Rgb24, 1, 1).ok()?;
    let context = cairo::Context::new(&measure).ok()?;
    font(&context);
    let extents = context.text_extents(text).ok()?;
    let font_extents = context.font_extents().ok()?;
    let width = (extents.x_advance() + 2.0 * PADDING).ceil() as i32;
    let height = (font_extents.height() + 2.0 * PADDING).ceil() as i32;

    let mut surface = cairo::ImageSurface::create(cairo::Format::Rgb24, width, height).ok()?;
    {
        let context = cairo::Context::new(&surface).ok()?;
        context.set_source_rgb(BACKGROUND.0, BACKGROUND.1, BACKGROUND.2);
        context.paint().ok()?;
        font(&context);
        context.set_source_rgb(TEXT.0, TEXT.1, TEXT.2);
        context.move_to(PADDING, PADDING + font_extents.ascent());
        context.show_text(text).ok()?;
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().ok()?;
    let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
    for row in data.chunks_exact(stride).take(height as usize) {
        // RGB24 is one native-endian 32-bit word per pixel: 0x00RRGGBB.
        for pixel in row[..width as usize * 4].chunks_exact(4) {
            let word = u32::from_ne_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]);
            ppm.extend_from_slice(&[(word >> 16) as u8, (word >> 8) as u8, word as u8]);
        }
    }
    Some(ppm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drawn_badge_is_a_ppm_of_its_own_size() {
        let ppm = draw("Miami, US").expect("a picture");
        let header_end = ppm
            .iter()
            .enumerate()
            .filter(|(_, byte)| **byte == b'\n')
            .nth(2)
            .map(|(at, _)| at + 1)
            .expect("a three-line header");
        let header = std::str::from_utf8(&ppm[..header_end]).unwrap();
        let mut fields = header.split_whitespace();
        assert_eq!(fields.next(), Some("P6"));
        let width: usize = fields.next().unwrap().parse().unwrap();
        let height: usize = fields.next().unwrap().parse().unwrap();
        assert_eq!(fields.next(), Some("255"));
        assert_eq!(ppm.len() - header_end, width * height * 3);
        // Some text was drawn: not every pixel is the plate's colour.
        let plate = (BACKGROUND.0 * 255.0).round() as u8;
        assert!(
            ppm[header_end..]
                .iter()
                .any(|byte| byte.abs_diff(plate) > 64)
        );
    }
}

