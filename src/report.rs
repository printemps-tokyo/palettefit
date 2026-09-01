//! Human-readable terminal output, with true-color swatches where supported.

use crate::check::{CheckReport, ColorStatus, Verdict};
use crate::color::Rgb8;
use crate::quantize::Cluster;
use std::fmt::Write as _;

/// Whether to emit ANSI escapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    Auto,
    Never,
}

pub struct Style {
    color: bool,
}

impl Style {
    pub fn new(choice: ColorChoice, stdout_is_tty: bool) -> Self {
        let color = match choice {
            ColorChoice::Never => false,
            ColorChoice::Auto => stdout_is_tty && std::env::var_os("NO_COLOR").is_none(),
        };
        Self { color }
    }

    /// A two-cell block painted with the color, so hex values can be checked
    /// by eye. Empty when color is off.
    pub fn swatch(&self, c: Rgb8) -> String {
        if self.color {
            format!("\x1b[48;2;{};{};{}m  \x1b[0m ", c.r, c.g, c.b)
        } else {
            String::new()
        }
    }

    fn paint(&self, code: &str, s: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    fn status_word(&self, s: ColorStatus) -> String {
        match s {
            ColorStatus::Ok => self.paint("32", "ok     "),
            ColorStatus::Drift => self.paint("33", "drift  "),
            ColorStatus::Missing => self.paint("31", "missing"),
        }
    }

    pub fn verdict_word(&self, v: Verdict) -> String {
        match v {
            Verdict::Pass => self.paint("32;1", "PASS"),
            Verdict::Warn => self.paint("33;1", "WARN"),
            Verdict::Fail => self.paint("31;1", "FAIL"),
        }
    }

    /// Render an extracted palette as a table.
    pub fn palette_table(&self, clusters: &[Cluster]) -> String {
        let mut out = String::new();
        for c in clusters {
            let _ = writeln!(
                out,
                "  {}#{}  {:>5.1}%",
                self.swatch(c.color),
                c.color.hex(),
                c.weight * 100.0
            );
        }
        out
    }

    /// Render one image's check report.
    pub fn check_table(&self, name: &str, report: &CheckReport) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "{name}: {}", self.verdict_word(report.verdict));
        for c in &report.colors {
            let label = c.label.as_deref().unwrap_or("");
            let nearest = match (c.nearest, c.delta_e) {
                (Some(n), Some(d)) => {
                    format!("nearest {}#{}  dE {:>5.2}", self.swatch(n), n.hex(), d)
                }
                _ => "no foreground pixels".to_string(),
            };
            let _ = writeln!(
                out,
                "  {} {}#{}  {}  {}",
                self.status_word(c.status),
                self.swatch(c.reference),
                c.reference.hex(),
                nearest,
                label,
            );
        }
        if !report.foreign.is_empty() {
            let _ = writeln!(
                out,
                "  foreign paint: {:.1}% of foreground",
                report.foreign_share * 100.0
            );
            for f in report.foreign.iter().take(5) {
                let _ = writeln!(
                    out,
                    "    {}#{}  {:>5.1}%  (nearest reference #{} at dE {:.2})",
                    self.swatch(f.color),
                    f.color.hex(),
                    f.weight * 100.0,
                    f.nearest_reference.hex(),
                    f.delta_e
                );
            }
        }
        out
    }
}
