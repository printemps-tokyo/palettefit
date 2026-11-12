//! WCAG 2.2 contrast ratios between palette colors.
//!
//! Relative luminance and contrast ratio follow the WCAG 2.2 definitions
//! (<https://www.w3.org/TR/WCAG22/#dfn-relative-luminance>, read
//! 2026-11-12): sRGB channels linearized with the 0.04045 threshold,
//! `L = 0.2126 R + 0.7152 G + 0.0722 B`, and `(L1 + 0.05) / (L2 + 0.05)` with
//! L1 the lighter color. The levels are the "at least" minimums of success
//! criteria 1.4.3 (AA: 4.5:1, large text 3:1), 1.4.6 (AAA: 7:1, large text
//! 4.5:1) and 1.4.11 (non-text: 3:1). Ratios are compared unrounded.

use crate::color::Rgb8;
use crate::palette::Palette;

/// WCAG 2.2 relative luminance of an sRGB color, 0..=1.
pub fn relative_luminance(c: Rgb8) -> f64 {
    let lin = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// WCAG contrast ratio, 1..=21.
pub fn contrast_ratio(a: Rgb8, b: Rgb8) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// A minimum a pair can be held to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// 1.4.3, normal text: 4.5:1.
    Aa,
    /// 1.4.3, large text: 3:1.
    AaLarge,
    /// 1.4.6, normal text: 7:1.
    Aaa,
    /// 1.4.6, large text: 4.5:1.
    AaaLarge,
    /// 1.4.11, user interface components and graphics: 3:1.
    NonText,
}

impl Level {
    pub const ALL: [Level; 5] = [
        Level::Aa,
        Level::AaLarge,
        Level::Aaa,
        Level::AaaLarge,
        Level::NonText,
    ];

    pub fn parse(s: &str) -> Result<Self, String> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "aa" => Level::Aa,
            "aa-large" => Level::AaLarge,
            "aaa" => Level::Aaa,
            "aaa-large" => Level::AaaLarge,
            "non-text" | "ui" => Level::NonText,
            _ => {
                return Err(format!(
                    "unknown level '{s}' (aa, aa-large, aaa, aaa-large, non-text)"
                ))
            }
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Level::Aa => "aa",
            Level::AaLarge => "aa-large",
            Level::Aaa => "aaa",
            Level::AaaLarge => "aaa-large",
            Level::NonText => "non-text",
        }
    }

    pub fn minimum(self) -> f64 {
        match self {
            Level::Aa | Level::AaaLarge => 4.5,
            Level::AaLarge | Level::NonText => 3.0,
            Level::Aaa => 7.0,
        }
    }
}

/// One pair's result.
#[derive(Debug, Clone)]
pub struct PairResult {
    pub a: Rgb8,
    pub b: Rgb8,
    pub a_label: Option<String>,
    pub b_label: Option<String>,
    pub ratio: f64,
    /// The levels this pair meets, in `Level::ALL` order.
    pub meets: Vec<Level>,
}

impl PairResult {
    fn new(a: (Rgb8, Option<String>), b: (Rgb8, Option<String>)) -> Self {
        let ratio = contrast_ratio(a.0, b.0);
        Self {
            a: a.0,
            b: b.0,
            a_label: a.1,
            b_label: b.1,
            ratio,
            meets: Level::ALL
                .into_iter()
                .filter(|l| ratio >= l.minimum())
                .collect(),
        }
    }

    pub fn meets_level(&self, level: Level) -> bool {
        self.meets.contains(&level)
    }
}

/// Resolve a color reference: a palette label, or a hex value (in the palette
/// or not).
fn resolve(palette: &Palette, key: &str) -> Result<(Rgb8, Option<String>), String> {
    let key = key.trim();
    if let Some(c) = palette
        .colors
        .iter()
        .find(|c| c.label.as_deref() == Some(key))
    {
        return Ok((c.color, c.label.clone()));
    }
    let rgb = Rgb8::parse(key)
        .ok_or_else(|| format!("'{key}' is neither a label in the palette nor a hex color"))?;
    let label = palette
        .colors
        .iter()
        .find(|c| c.color == rgb)
        .and_then(|c| c.label.clone());
    Ok((rgb, label))
}

/// Parse `fg:bg,fg:bg` into pairs.
pub fn pairs(palette: &Palette, spec: &str) -> Result<Vec<PairResult>, String> {
    let mut out = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = part
            .split_once(':')
            .ok_or_else(|| format!("pair '{part}' should be written fg:bg"))?;
        out.push(PairResult::new(resolve(palette, a)?, resolve(palette, b)?));
    }
    if out.is_empty() {
        return Err("no pairs given".to_string());
    }
    Ok(out)
}

/// Every pair of distinct palette colors, highest ratio first (ties in
/// palette order).
pub fn all_pairs(palette: &Palette) -> Vec<PairResult> {
    let mut out = Vec::new();
    for (i, a) in palette.colors.iter().enumerate() {
        for b in &palette.colors[i + 1..] {
            out.push(PairResult::new(
                (a.color, a.label.clone()),
                (b.color, b.label.clone()),
            ));
        }
    }
    out.sort_by(|x, y| y.ratio.total_cmp(&x.ratio));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(hex: &str) -> Rgb8 {
        Rgb8::parse(hex).unwrap()
    }

    #[test]
    fn known_ratios() {
        assert!((contrast_ratio(rgb("000000"), rgb("ffffff")) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio(rgb("777777"), rgb("777777")) - 1.0).abs() < 1e-12);
        // #767676 on white is the usual "just passes AA" grey (about 4.54:1),
        // #777777 just fails (about 4.48:1).
        let pass = contrast_ratio(rgb("767676"), rgb("ffffff"));
        let fail = contrast_ratio(rgb("777777"), rgb("ffffff"));
        assert!((4.5..4.6).contains(&pass), "{pass}");
        assert!((4.4..4.5).contains(&fail), "{fail}");
        assert!((relative_luminance(rgb("ffffff")) - 1.0).abs() < 1e-12);
        assert_eq!(relative_luminance(rgb("000000")), 0.0);
    }

    #[test]
    fn levels_and_pairs() {
        let palette = Palette::parse("ffffff 0.5 paper\n1d3557 0.3 ink\n777777\n").unwrap();
        let p = pairs(&palette, "ink:paper, 777777:paper").unwrap();
        assert!(p[0].meets_level(Level::Aaa));
        assert_eq!(p[0].a_label.as_deref(), Some("ink"));
        assert!(!p[1].meets_level(Level::Aa) && p[1].meets_level(Level::AaLarge));
        assert!(pairs(&palette, "ink").is_err());
        assert!(pairs(&palette, "nope:paper").is_err());
        let all = all_pairs(&palette);
        assert_eq!(all.len(), 3);
        assert!(all[0].ratio >= all[1].ratio && all[1].ratio >= all[2].ratio);
        assert_eq!(Level::parse("UI").unwrap(), Level::NonText);
        assert!(Level::parse("a").is_err());
    }
}
