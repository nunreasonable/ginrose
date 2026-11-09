//! Where the Roblox server the player joined is, shown for a few seconds over
//! the game. Ginrose draws it into a small picture that Ginrose's Rosemary
//! slides over every frame (`ROSEMARY_OVERLAY_IMAGE`). Opt-in: the place comes
//! from asking ipinfo.io about the server's address.

use std::f64::consts::PI;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use gtk::cairo;
use serde_json::Value;

use crate::config::Config;
use crate::paths;

/// The positions Rosemary accepts, and what the settings call them.
pub const POSITIONS: [(&str, &str); 6] = [
    ("top", "Top"),
    ("bottom", "Bottom"),
    ("top-left", "Top left"),
    ("top-right", "Top right"),
    ("bottom-left", "Bottom left"),
    ("bottom-right", "Bottom right"),
];

pub const DEFAULT_POSITION: &str = "top";

/// How long the notice stays before it slides away.
const SHOWN_FOR: Duration = Duration::from_secs(6);

const FONT_SIZE: f64 = 15.0;
const PADDING_X: f64 = 14.0;
const PADDING_Y: f64 = 10.0;
const RADIUS: f64 = 10.0;
const ICON: f64 = 20.0;
const ICON_RADIUS: f64 = 4.0;
const ICON_GAP: f64 = 12.0;
const PLATE: (f64, f64, f64) = (0.12, 0.13, 0.16);
const ICON_FILL: (f64, f64, f64) = (0.21, 0.52, 0.89);
const TEXT: (f64, f64, f64) = (0.93, 0.93, 0.95);

/// The notice being shown; a newer one keeps an older one's timer from
/// hiding it.
static SHOWING: AtomicU64 = AtomicU64::new(0);

/// Where `name` is in [`POSITIONS`], or the default's place.
pub fn position_index(name: &str) -> u32 {
    POSITIONS
        .iter()
        .position(|(position, _)| *position == name)
        .unwrap_or_default() as u32
}

/// The position at `index` in [`POSITIONS`], or the default.
pub fn position_name(index: u32) -> &'static str {
    POSITIONS
        .get(index as usize)
        .map_or(DEFAULT_POSITION, |(name, _)| name)
}

fn notice_path() -> PathBuf {
    paths::runtime_dir().join("ginrose-server-region.pam")
}

/// Ask Rosemary to show the notice, starting with none.
pub fn environment(config: &Config, command: &mut Command) {
    if !config.region_badge {
        return;
    }
    let path = notice_path();
    let _ = std::fs::write(&path, b"");
    command.env("ROSEMARY_OVERLAY_IMAGE", path);
    command.env(
        "ROSEMARY_OVERLAY_POSITION",
        position_name(position_index(&config.region_position)),
    );
}

/// "City, Region, Country" for a server's public address.
pub fn locate(address: &str) -> Option<String> {
    let info: Value = crate::roblox::agent()
        .get(&format!("https://ipinfo.io/{address}/json"))
        .call()
        .ok()?
        .body_mut()
        .read_json()
        .ok()?;
    let mut parts: Vec<&str> = Vec::new();
    for key in ["city", "region", "country"] {
        if let Some(part) = info[key].as_str().filter(|part| !part.is_empty())
            && !parts.contains(&part)
        {
            parts.push(part);
        }
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// Show where the server is, then hide it after [`SHOWN_FOR`].
pub fn announce(place: &str) {
    let showing = SHOWING.fetch_add(1, Ordering::Relaxed) + 1;
    write(draw(&format!("Server location: {place}")));
    std::thread::sleep(SHOWN_FOR);
    if SHOWING.load(Ordering::Relaxed) == showing {
        hide();
    }
}

/// Take the notice away.
pub fn hide() {
    SHOWING.fetch_add(1, Ordering::Relaxed);
    write(None);
}

/// Replace the file whole, so Rosemary never reads half a picture.
fn write(picture: Option<Vec<u8>>) {
    let path = notice_path();
    let partial = path.with_extension("part");
    if std::fs::write(&partial, picture.unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&partial, &path);
    }
}

fn rounded(context: &cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    context.new_sub_path();
    context.arc(x + width - radius, y + radius, radius, -PI / 2.0, 0.0);
    context.arc(
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        PI / 2.0,
    );
    context.arc(x + radius, y + height - radius, radius, PI / 2.0, PI);
    context.arc(x + radius, y + radius, radius, PI, 3.0 * PI / 2.0);
    context.close_path();
}

/// `text` beside an information mark on a rounded plate, as an RGB_ALPHA PAM.
fn draw(text: &str) -> Option<Vec<u8>> {
    let font = |context: &cairo::Context, weight| {
        context.select_font_face("Sans", cairo::FontSlant::Normal, weight);
        context.set_font_size(FONT_SIZE);
    };
    let measure = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).ok()?;
    let context = cairo::Context::new(&measure).ok()?;
    font(&context, cairo::FontWeight::Normal);
    let advance = context.text_extents(text).ok()?.x_advance();
    let line = context.font_extents().ok()?;
    let height = (line.height().max(ICON) + 2.0 * PADDING_Y).ceil();
    let width = (PADDING_X + ICON + ICON_GAP + advance + PADDING_X).ceil();

    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, width as i32, height as i32).ok()?;
    {
        let context = cairo::Context::new(&surface).ok()?;
        rounded(&context, 0.0, 0.0, width, height, RADIUS);
        context.set_source_rgb(PLATE.0, PLATE.1, PLATE.2);
        context.fill().ok()?;

        let icon_y = (height - ICON) / 2.0;
        rounded(&context, PADDING_X, icon_y, ICON, ICON, ICON_RADIUS);
        context.set_source_rgb(ICON_FILL.0, ICON_FILL.1, ICON_FILL.2);
        context.fill().ok()?;
        font(&context, cairo::FontWeight::Bold);
        let mark = context.text_extents("i").ok()?;
        context.set_source_rgb(1.0, 1.0, 1.0);
        context.move_to(
            PADDING_X + (ICON - mark.width()) / 2.0 - mark.x_bearing(),
            icon_y + (ICON - mark.height()) / 2.0 - mark.y_bearing(),
        );
        context.show_text("i").ok()?;

        font(&context, cairo::FontWeight::Normal);
        context.set_source_rgb(TEXT.0, TEXT.1, TEXT.2);
        let baseline = (height - line.height()) / 2.0 + line.ascent();
        context.move_to(PADDING_X + ICON + ICON_GAP, baseline);
        context.show_text(text).ok()?;
    }
    surface.flush();
    let (width, height) = (width as usize, height as usize);
    let stride = surface.stride() as usize;
    let data = surface.data().ok()?;
    let mut pam = format!(
        "P7\nWIDTH {width}\nHEIGHT {height}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n"
    )
    .into_bytes();
    for row in data.chunks_exact(stride).take(height) {
        // ARGB32 is one native-endian 32-bit word per pixel: 0xAARRGGBB,
        // premultiplied, which only the antialiased edges show.
        for pixel in row[..width * 4].chunks_exact(4) {
            let word = u32::from_ne_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]);
            pam.extend_from_slice(&[
                (word >> 16) as u8,
                (word >> 8) as u8,
                word as u8,
                (word >> 24) as u8,
            ]);
        }
    }
    Some(pam)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drawn_notice_is_a_pam_of_its_own_size_with_clear_corners() {
        let pam = draw("Server location: Miami, Florida, US").expect("a picture");
        let end = pam
            .windows(7)
            .position(|w| w == b"ENDHDR\n")
            .expect("a header")
            + 7;
        let header = std::str::from_utf8(&pam[..end]).unwrap();
        let field = |key: &str| -> usize {
            header
                .lines()
                .find_map(|line| line.strip_prefix(key))
                .unwrap()
                .trim()
                .parse()
                .unwrap()
        };
        let (width, height) = (field("WIDTH"), field("HEIGHT"));
        assert_eq!(pam.len() - end, width * height * 4);
        let alpha = |x: usize, y: usize| pam[end + (y * width + x) * 4 + 3];
        // The rounded plate leaves its corners clear and its middle solid.
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(width - 1, height - 1), 0);
        assert_eq!(alpha(width / 2, height / 2), 255);
    }

    #[test]
    fn an_unknown_position_falls_back_to_the_top() {
        assert_eq!(position_name(position_index("top-right")), "top-right");
        assert_eq!(position_name(position_index("middle")), DEFAULT_POSITION);
    }
}
