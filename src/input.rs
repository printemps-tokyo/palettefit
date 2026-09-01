//! Image loading, pixel sampling and background exclusion.

use crate::color::{delta_e_rgb, Rgb8};
use crate::quantize::histogram;
use anyhow::{Context, Result};
use std::path::Path;

/// Pixels with alpha below this are treated as fully transparent background.
const ALPHA_MIN: u8 = 128;

/// Sampling never reads more than about this many pixels; larger images are
/// walked on a fixed grid. Deterministic: the step depends only on the size.
const MAX_SAMPLES: u64 = 262_144;

/// Background colors are excluded when they sit within this CIEDE2000
/// distance of the detected/declared background color. JPEG artifacts around
/// a flat background typically stay well inside this.
const BG_TOLERANCE: f64 = 4.0;

/// How the background should be excluded before palette extraction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BgMode {
    /// Detect a flat background from the image corners (default). Transparent
    /// pixels are always excluded regardless of mode.
    Auto,
    /// Only transparent pixels are excluded.
    None,
    /// Treat this color (and colors within tolerance of it) as background.
    Color(Rgb8),
}

impl BgMode {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "auto" => Ok(Self::Auto),
            "none" => Ok(Self::None),
            other => Rgb8::parse(other).map(Self::Color).with_context(|| {
                format!("invalid --bg value {other:?}: expected auto, none, or a hex color")
            }),
        }
    }
}

/// What was sampled from one image.
#[derive(Debug)]
pub struct SampledImage {
    /// Unique opaque, non-background colors with sample counts.
    pub entries: Vec<(Rgb8, u64)>,
    /// The background color that was excluded, when one was.
    pub background: Option<Rgb8>,
    /// Fraction of sampled pixels that were transparent or background.
    pub excluded_share: f64,
    pub width: u32,
    pub height: u32,
}

/// Load an image and sample its opaque foreground colors.
pub fn sample(path: &Path, bg: BgMode) -> Result<SampledImage> {
    let img = image::open(path)
        .with_context(|| format!("cannot read image {}", path.display()))?
        .to_rgba8();
    let (width, height) = img.dimensions();

    // Grid step so that step*step buckets keep us near MAX_SAMPLES.
    let total = u64::from(width) * u64::from(height);
    let step = ((total as f64 / MAX_SAMPLES as f64).sqrt().ceil() as u32).max(1);

    let mut sampled = 0u64;
    let mut transparent = 0u64;
    let mut opaque: Vec<Rgb8> = Vec::new();
    let mut y = 0;
    while y < height {
        let mut x = 0;
        while x < width {
            let p = img.get_pixel(x, y);
            sampled += 1;
            if p[3] < ALPHA_MIN {
                transparent += 1;
            } else {
                opaque.push(Rgb8::new(p[0], p[1], p[2]));
            }
            x += step;
        }
        y += step;
    }

    let background = match bg {
        BgMode::None => None,
        BgMode::Color(c) => Some(c),
        BgMode::Auto => detect_background(&img, width, height),
    };

    let mut entries = histogram(opaque);
    let mut excluded = transparent;
    if let Some(bg_color) = background {
        entries.retain(|&(c, n)| {
            if delta_e_rgb(c, bg_color) <= BG_TOLERANCE {
                excluded += n;
                false
            } else {
                true
            }
        });
    }

    Ok(SampledImage {
        entries,
        background,
        excluded_share: if sampled == 0 {
            0.0
        } else {
            excluded as f64 / sampled as f64
        },
        width,
        height,
    })
}

/// A flat background shows the same color in most corners. Require at least
/// three of the four corners to agree within tolerance; otherwise assume the
/// artwork bleeds to the edges and remove nothing.
fn detect_background(img: &image::RgbaImage, width: u32, height: u32) -> Option<Rgb8> {
    let corners = [
        (0, 0),
        (width - 1, 0),
        (0, height - 1),
        (width - 1, height - 1),
    ];
    let colors: Vec<Option<Rgb8>> = corners
        .iter()
        .map(|&(x, y)| {
            let p = img.get_pixel(x, y);
            (p[3] >= ALPHA_MIN).then(|| Rgb8::new(p[0], p[1], p[2]))
        })
        .collect();
    for candidate in colors.iter().flatten() {
        let agreeing = colors
            .iter()
            .flatten()
            .filter(|c| delta_e_rgb(**c, *candidate) <= BG_TOLERANCE)
            .count();
        if agreeing >= 3 {
            return Some(*candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn write_png(dir: &Path, name: &str, img: &RgbaImage) -> std::path::PathBuf {
        let p = dir.join(name);
        img.save(&p).unwrap();
        p
    }

    #[test]
    fn white_background_is_detected_and_excluded() {
        let dir = tempfile::tempdir().unwrap();
        let mut img = RgbaImage::from_pixel(40, 40, Rgba([255, 255, 255, 255]));
        for y in 10..30 {
            for x in 10..30 {
                img.put_pixel(x, y, Rgba([230, 57, 70, 255]));
            }
        }
        let path = write_png(dir.path(), "a.png", &img);
        let s = sample(&path, BgMode::Auto).unwrap();
        assert_eq!(s.background, Some(Rgb8::new(255, 255, 255)));
        assert_eq!(s.entries.len(), 1);
        assert_eq!(s.entries[0].0, Rgb8::new(230, 57, 70));
        assert!(s.excluded_share > 0.5);
    }

    #[test]
    fn bg_none_keeps_the_background_color() {
        let dir = tempfile::tempdir().unwrap();
        let img = RgbaImage::from_pixel(8, 8, Rgba([255, 255, 255, 255]));
        let path = write_png(dir.path(), "b.png", &img);
        let s = sample(&path, BgMode::None).unwrap();
        assert_eq!(s.background, None);
        assert_eq!(s.entries.len(), 1);
    }

    #[test]
    fn transparent_pixels_are_always_excluded() {
        let dir = tempfile::tempdir().unwrap();
        let mut img = RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 0]));
        for x in 0..10 {
            img.put_pixel(x, 5, Rgba([69, 123, 157, 255]));
        }
        let path = write_png(dir.path(), "c.png", &img);
        let s = sample(&path, BgMode::None).unwrap();
        assert_eq!(s.entries.len(), 1);
        assert_eq!(s.entries[0].0, Rgb8::new(69, 123, 157));
    }

    #[test]
    fn artwork_to_the_edges_detects_no_background() {
        let dir = tempfile::tempdir().unwrap();
        let mut img = RgbaImage::new(4, 4);
        // Four clearly different corner colors.
        let cs = [
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 255, 0, 255],
        ];
        for (i, p) in img.pixels_mut().enumerate() {
            *p = Rgba(cs[i % 4]);
        }
        let path = write_png(dir.path(), "d.png", &img);
        let s = sample(&path, BgMode::Auto).unwrap();
        assert_eq!(s.background, None);
    }
}
