//! The palette file: a plain-text, hand-editable list of reference colors.
//!
//! ```text
//! # palettefit palette v1
//! # name: Miko
//! # source: miko-ref-sheet.png
//! e63946 0.412 hair
//! 457b9d 0.288 jacket
//! f1faee 0.183 skin
//! 1d3557        eyes
//! ```
//!
//! One color per line: hex, an optional weight (the share of foreground
//! pixels it covered in the reference), and an optional free-text label.
//! Lines starting with `#` are comments; `# name:` and `# source:` are
//! recognized as metadata.

use crate::color::Rgb8;
use crate::quantize::Cluster;
use anyhow::{bail, Context, Result};
use std::fmt::Write as _;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct PaletteColor {
    pub color: Rgb8,
    pub weight: Option<f64>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Palette {
    pub name: Option<String>,
    pub source: Option<String>,
    pub colors: Vec<PaletteColor>,
}

impl Palette {
    pub fn from_clusters(clusters: &[Cluster], name: Option<&str>, source: Option<&str>) -> Self {
        Self {
            name: name.map(str::to_string),
            source: source.map(str::to_string),
            colors: clusters
                .iter()
                .map(|c| PaletteColor {
                    color: c.color,
                    weight: Some(c.weight),
                    label: None,
                })
                .collect(),
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut palette = Palette::default();
        for (lineno, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            let mut tokens = line.split_whitespace();
            let hex = tokens.next().expect("non-empty line has a first token");
            // A leading '#' can open either a comment or a `#rrggbb` color, so
            // try the color reading first: `#1d3557 eyes` is a color line,
            // `# name: Miko` is metadata.
            let color = match Rgb8::parse(hex) {
                Some(c) => c,
                None if line.starts_with('#') => {
                    let comment = line[1..].trim();
                    if let Some(v) = comment.strip_prefix("name:") {
                        palette.name = Some(v.trim().to_string());
                    } else if let Some(v) = comment.strip_prefix("source:") {
                        palette.source = Some(v.trim().to_string());
                    }
                    continue;
                }
                None => {
                    bail!("line {}: {hex:?} is not a hex color", lineno + 1);
                }
            };
            let rest: Vec<&str> = tokens.collect();
            let (weight, label_tokens) = match rest.first().and_then(|t| t.parse::<f64>().ok()) {
                Some(w) => {
                    if !(0.0..=1.0).contains(&w) {
                        bail!("line {}: weight {w} is outside 0..=1", lineno + 1);
                    }
                    (Some(w), &rest[1..])
                }
                None => (None, &rest[..]),
            };
            let label = if label_tokens.is_empty() {
                None
            } else {
                Some(label_tokens.join(" "))
            };
            palette.colors.push(PaletteColor {
                color,
                weight,
                label,
            });
        }
        if palette.colors.is_empty() {
            bail!("palette file contains no colors");
        }
        Ok(palette)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read palette {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("invalid palette {}", path.display()))
    }

    /// Serialize back to the palette file format.
    pub fn to_text(&self) -> String {
        let mut out = String::from("# palettefit palette v1\n");
        if let Some(name) = &self.name {
            let _ = writeln!(out, "# name: {name}");
        }
        if let Some(source) = &self.source {
            let _ = writeln!(out, "# source: {source}");
        }
        for c in &self.colors {
            let _ = write!(out, "{}", c.color.hex());
            if let Some(w) = c.weight {
                let _ = write!(out, " {w:.4}");
            }
            if let Some(label) = &c.label {
                let _ = write!(out, " {label}");
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_file() {
        let text = "# palettefit palette v1\n# name: Miko\n# source: ref.png\n\
                    e63946 0.412 hair\n457b9d 0.288 warm jacket\n#1d3557 eyes\nf1faee\n";
        let p = Palette::parse(text).unwrap();
        assert_eq!(p.name.as_deref(), Some("Miko"));
        assert_eq!(p.source.as_deref(), Some("ref.png"));
        assert_eq!(p.colors.len(), 4);
        assert_eq!(p.colors[0].color.hex(), "e63946");
        assert_eq!(p.colors[0].weight, Some(0.412));
        assert_eq!(p.colors[1].label.as_deref(), Some("warm jacket"));
        assert_eq!(p.colors[2].weight, None);
        assert_eq!(p.colors[2].label.as_deref(), Some("eyes"));
        assert_eq!(p.colors[3].label, None);
    }

    #[test]
    fn roundtrip() {
        let text = "# palettefit palette v1\n# name: X\ne63946 0.5000 hair\n457b9d\n";
        let p = Palette::parse(text).unwrap();
        let p2 = Palette::parse(&p.to_text()).unwrap();
        assert_eq!(p2.colors.len(), 2);
        assert_eq!(p2.name.as_deref(), Some("X"));
        assert_eq!(p2.colors[0].weight, Some(0.5));
    }

    #[test]
    fn rejects_bad_lines() {
        assert!(Palette::parse("zzz 0.5\n").is_err());
        assert!(Palette::parse("e63946 1.5\n").is_err());
        assert!(Palette::parse("# only comments\n").is_err());
    }
}
