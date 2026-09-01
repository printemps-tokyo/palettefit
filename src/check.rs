//! The audit: score an image's extracted clusters against a reference palette.

use crate::color::{delta_e_rgb, Rgb8};
use crate::palette::Palette;
use crate::quantize::Cluster;

/// Default CIEDE2000 threshold below which a reference color counts as held.
/// Extraction itself shifts cluster means a little, so this sits above the
/// perceptibility range on purpose.
pub const DEFAULT_WARN_DELTA: f64 = 3.0;
/// Default threshold beyond which a reference color counts as missing.
pub const DEFAULT_FAIL_DELTA: f64 = 8.0;

/// How many clusters to extract from a checked image: enough that every
/// reference color has a chance to surface as its own cluster.
pub fn extract_k(palette_len: usize) -> usize {
    (palette_len * 2).clamp(12, 32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorStatus {
    /// Nearest cluster within the warn threshold.
    Ok,
    /// Between warn and fail: the color is there but has drifted.
    Drift,
    /// No cluster anywhere near: the color is gone or replaced.
    Missing,
}

impl ColorStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Drift => "drift",
            Self::Missing => "missing",
        }
    }
}

/// Verdict for one reference color.
#[derive(Debug, Clone)]
pub struct ColorResult {
    pub reference: Rgb8,
    pub label: Option<String>,
    pub weight: Option<f64>,
    /// The closest extracted cluster, if the image had any foreground at all.
    pub nearest: Option<Rgb8>,
    pub delta_e: Option<f64>,
    pub status: ColorStatus,
}

/// A dominant cluster in the checked image that is far from every reference
/// color: paint that should not be there.
#[derive(Debug, Clone)]
pub struct ForeignColor {
    pub color: Rgb8,
    pub weight: f64,
    pub nearest_reference: Rgb8,
    pub delta_e: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Warn,
    Fail,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckReport {
    pub colors: Vec<ColorResult>,
    pub foreign: Vec<ForeignColor>,
    /// Weighted share of extracted clusters that are foreign.
    pub foreign_share: f64,
    pub verdict: Verdict,
}

pub struct Thresholds {
    pub warn_delta: f64,
    pub fail_delta: f64,
    /// When set, a foreign share above this makes the image fail.
    pub max_foreign: Option<f64>,
    /// Treat drift (warn) as failure.
    pub strict: bool,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            warn_delta: DEFAULT_WARN_DELTA,
            fail_delta: DEFAULT_FAIL_DELTA,
            max_foreign: None,
            strict: false,
        }
    }
}

pub fn check(palette: &Palette, clusters: &[Cluster], t: &Thresholds) -> CheckReport {
    let colors: Vec<ColorResult> = palette
        .colors
        .iter()
        .map(|pc| {
            let nearest = clusters
                .iter()
                .map(|c| (c.color, delta_e_rgb(pc.color, c.color)))
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            let (nearest, delta_e) = match nearest {
                Some((c, d)) => (Some(c), Some(d)),
                None => (None, None),
            };
            let status = match delta_e {
                Some(d) if d <= t.warn_delta => ColorStatus::Ok,
                Some(d) if d <= t.fail_delta => ColorStatus::Drift,
                _ => ColorStatus::Missing,
            };
            ColorResult {
                reference: pc.color,
                label: pc.label.clone(),
                weight: pc.weight,
                nearest,
                delta_e,
                status,
            }
        })
        .collect();

    let mut foreign: Vec<ForeignColor> = clusters
        .iter()
        .filter_map(|c| {
            let (nearest_reference, delta_e) = palette
                .colors
                .iter()
                .map(|pc| (pc.color, delta_e_rgb(c.color, pc.color)))
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))?;
            (delta_e > t.fail_delta).then_some(ForeignColor {
                color: c.color,
                weight: c.weight,
                nearest_reference,
                delta_e,
            })
        })
        .collect();
    foreign.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.color.cmp(&b.color))
    });
    let foreign_share: f64 = foreign.iter().map(|f| f.weight).sum();

    let any_missing = colors.iter().any(|c| c.status == ColorStatus::Missing);
    let any_drift = colors.iter().any(|c| c.status == ColorStatus::Drift);
    let foreign_over = t.max_foreign.is_some_and(|m| foreign_share > m);
    let verdict = if any_missing || foreign_over || (t.strict && any_drift) {
        Verdict::Fail
    } else if any_drift {
        Verdict::Warn
    } else {
        Verdict::Pass
    };

    CheckReport {
        colors,
        foreign,
        foreign_share,
        verdict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::PaletteColor;

    fn palette_of(colors: &[(u8, u8, u8)]) -> Palette {
        Palette {
            name: None,
            source: None,
            colors: colors
                .iter()
                .map(|&(r, g, b)| PaletteColor {
                    color: Rgb8::new(r, g, b),
                    weight: None,
                    label: None,
                })
                .collect(),
        }
    }

    fn clusters_of(colors: &[((u8, u8, u8), f64)]) -> Vec<Cluster> {
        colors
            .iter()
            .map(|&((r, g, b), weight)| Cluster {
                color: Rgb8::new(r, g, b),
                weight,
            })
            .collect()
    }

    #[test]
    fn exact_match_passes() {
        let p = palette_of(&[(230, 57, 70), (69, 123, 157)]);
        let c = clusters_of(&[((230, 57, 70), 0.6), ((69, 123, 157), 0.4)]);
        let r = check(&p, &c, &Thresholds::default());
        assert_eq!(r.verdict, Verdict::Pass);
        assert!(r.foreign.is_empty());
        assert_eq!(r.colors[0].delta_e, Some(0.0));
    }

    #[test]
    fn replaced_color_is_missing_and_foreign() {
        let p = palette_of(&[(230, 57, 70), (69, 123, 157)]);
        // Blue jacket got recolored green.
        let c = clusters_of(&[((230, 57, 70), 0.6), ((60, 160, 60), 0.4)]);
        let r = check(&p, &c, &Thresholds::default());
        assert_eq!(r.verdict, Verdict::Fail);
        assert_eq!(r.colors[1].status, ColorStatus::Missing);
        assert_eq!(r.foreign.len(), 1);
        assert!((r.foreign_share - 0.4).abs() < 1e-9);
    }

    #[test]
    fn small_shift_warns_and_strict_fails_it() {
        let p = palette_of(&[(230, 57, 70)]);
        // A nudge of a few RGB steps: perceptible but present.
        let c = clusters_of(&[((222, 66, 82), 1.0)]);
        let base = check(&p, &c, &Thresholds::default());
        assert_eq!(base.colors[0].status, ColorStatus::Drift);
        assert_eq!(base.verdict, Verdict::Warn);
        let strict = check(
            &p,
            &c,
            &Thresholds {
                strict: true,
                ..Thresholds::default()
            },
        );
        assert_eq!(strict.verdict, Verdict::Fail);
    }

    #[test]
    fn max_foreign_gate() {
        let p = palette_of(&[(230, 57, 70)]);
        let c = clusters_of(&[((230, 57, 70), 0.7), ((20, 200, 20), 0.3)]);
        let lenient = check(&p, &c, &Thresholds::default());
        assert_eq!(lenient.verdict, Verdict::Pass);
        let gated = check(
            &p,
            &c,
            &Thresholds {
                max_foreign: Some(0.2),
                ..Thresholds::default()
            },
        );
        assert_eq!(gated.verdict, Verdict::Fail);
    }

    #[test]
    fn empty_image_reports_missing() {
        let p = palette_of(&[(230, 57, 70)]);
        let r = check(&p, &[], &Thresholds::default());
        assert_eq!(r.colors[0].status, ColorStatus::Missing);
        assert_eq!(r.verdict, Verdict::Fail);
    }
}
