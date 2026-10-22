//! What the tray icon shows: the current layout as two or three letters on a
//! colored square, grey while paused. The app only draws it.

use crate::layout::LayoutId;

/// Colors for configured layouts, in `layouts` order (blue, coral, teal,
/// purple, amber); white text reads well on all of them.
const PALETTE: [Rgb; 5] = [
    Rgb(0x18, 0x5F, 0xA5),
    Rgb(0x99, 0x3C, 0x1D),
    Rgb(0x0F, 0x6E, 0x56),
    Rgb(0x53, 0x4A, 0xB7),
    Rgb(0x85, 0x4F, 0x0B),
];
/// A layout that is not in `layouts`.
const OTHER: Rgb = Rgb(0x44, 0x44, 0x41);
const PAUSED: Rgb = Rgb(0x88, 0x87, 0x80);
const TEXT: Rgb = Rgb(0xFF, 0xFF, 0xFF);

/// An sRGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// The tray icon's content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Indicator {
    pub label: String,
    pub background: Rgb,
    pub foreground: Rgb,
    pub paused: bool,
}

impl Indicator {
    /// The indicator for the foreground layout (`None` if unknown).
    pub fn new(maybe_layout: Option<&LayoutId>, configured: &[LayoutId], paused: bool) -> Self {
        let label = maybe_layout.map_or_else(|| "--".to_owned(), short_label);
        let background = if paused {
            PAUSED
        } else {
            maybe_layout
                .and_then(|id| configured.iter().position(|c| c == id))
                .map_or(OTHER, |i| PALETTE[i % PALETTE.len()])
        };
        Self {
            label,
            background,
            foreground: TEXT,
            paused,
        }
    }

    /// Tray tooltip text.
    pub fn tooltip(&self) -> String {
        if self.paused {
            format!("retype — paused ({})", self.label)
        } else {
            format!("retype — {}", self.label)
        }
    }
}

/// The language part of a tag, upper-cased, at most three letters:
/// `en-us` → `EN`, `fil-ph` → `FIL`.
pub fn short_label(id: &LayoutId) -> String {
    id.as_str()
        .split('-')
        .next()
        .unwrap_or_default()
        .chars()
        .take(3)
        .collect::<String>()
        .to_ascii_uppercase()
}

/// Alpha mask (0–255, row-major, `size`×`size`) of a square with rounded
/// corners, anti-aliased by 4×4 supersampling.
pub fn rounded_square_alpha(size: u32, radius: f32) -> Vec<u8> {
    const SAMPLES: u32 = 4;
    let size_f = size as f32;
    let radius = radius.clamp(0.0, size_f / 2.0);
    let inside = |x: f32, y: f32| {
        // Distance outside the inner rectangle the corners are drawn around.
        let dx = (radius - x).max(x - (size_f - radius)).max(0.0);
        let dy = (radius - y).max(y - (size_f - radius)).max(0.0);
        dx * dx + dy * dy <= radius * radius
    };
    let mut alpha = Vec::with_capacity((size * size) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut hits = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = px as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                    let y = py as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
                    hits += u32::from(inside(x, y));
                }
            }
            alpha.push((hits * 255 / (SAMPLES * SAMPLES)) as u8);
        }
    }
    alpha
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(short_label(&id("en-US")), "EN");
        assert_eq!(short_label(&id("ru-ru")), "RU");
        assert_eq!(short_label(&id("fil-PH")), "FIL");
        assert_eq!(short_label(&id("de")), "DE");
    }

    #[test]
    fn colors_follow_the_configured_order() {
        let configured = [id("en-US"), id("ru-RU")];
        let en = Indicator::new(Some(&id("en-US")), &configured, false);
        let ru = Indicator::new(Some(&id("ru-RU")), &configured, false);
        let de = Indicator::new(Some(&id("de-DE")), &configured, false);
        assert_eq!(en.background, PALETTE[0]);
        assert_eq!(ru.background, PALETTE[1]);
        assert_eq!(de.background, OTHER);
        assert_eq!(de.label, "DE");
    }

    #[test]
    fn paused_is_grey_and_says_so() {
        let configured = [id("en-US")];
        let paused = Indicator::new(Some(&id("en-US")), &configured, true);
        assert_eq!(paused.background, PAUSED);
        assert_eq!(paused.tooltip(), "retype — paused (EN)");
        let unknown = Indicator::new(None, &configured, false);
        assert_eq!(unknown.label, "--");
        assert_eq!(unknown.tooltip(), "retype — --");
    }

    #[test]
    fn rounded_square_mask() {
        let size = 16;
        let alpha = rounded_square_alpha(size, 4.0);
        let at = |x: u32, y: u32| alpha[(y * size + x) as usize];
        assert_eq!(alpha.len(), 256);
        assert_eq!(at(0, 0), 0, "corner is transparent");
        assert_eq!(at(15, 15), 0);
        assert_eq!(at(8, 8), 255, "center is opaque");
        assert_eq!(at(8, 0), 255, "edge middle is opaque");
        assert!(
            at(1, 1) > 0 && at(1, 1) < 255,
            "corner edge is anti-aliased"
        );
        assert!(rounded_square_alpha(8, 0.0).iter().all(|&a| a == 255));
    }

    fn id(tag: &str) -> LayoutId {
        LayoutId::new(tag).unwrap()
    }
}
