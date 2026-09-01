//! palettefit CLI: extract reference palettes, check images against them.

use anyhow::Result;
use clap::{Parser, Subcommand};
use palettefit::check::{self, CheckReport, Thresholds};
use palettefit::color::delta_e_rgb;
use palettefit::input::{sample, BgMode, SampledImage};
use palettefit::json;
use palettefit::palette::Palette;
use palettefit::quantize::{median_cut, Cluster};
use palettefit::report::{ColorChoice, Style};
use std::fmt::Write as _;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(
    name = "palettefit",
    version,
    about = "Audit character art and brand assets for color drift with CIEDE2000",
    after_help = "Exit codes: 0 pass (warnings allowed unless --strict), 1 fail, 2 error."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Extract a reference palette from an image (model sheet, key art)
    Extract {
        /// Reference image (png / jpeg / webp)
        image: PathBuf,
        /// Number of palette colors to extract
        #[arg(short = 'k', long, default_value_t = 8)]
        colors: usize,
        /// Background handling: auto, none, or a hex color
        #[arg(long, default_value = "auto", value_parser = BgMode::parse)]
        bg: BgMode,
        /// Character or asset name recorded in the palette file
        #[arg(long)]
        name: Option<String>,
        /// Write the palette file here instead of stdout
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Machine-readable JSON on stdout
        #[arg(long)]
        json: bool,
        /// Disable ANSI colors
        #[arg(long)]
        no_color: bool,
    },
    /// Check images against a reference palette
    Check {
        /// Images to audit
        #[arg(required = true)]
        images: Vec<PathBuf>,
        /// Palette file produced by `palettefit extract` (or hand-written)
        #[arg(short, long)]
        palette: PathBuf,
        /// CIEDE2000 distance up to which a reference color counts as held
        #[arg(long, default_value_t = check::DEFAULT_WARN_DELTA)]
        warn_delta: f64,
        /// CIEDE2000 distance beyond which a reference color counts as missing
        #[arg(long, default_value_t = check::DEFAULT_FAIL_DELTA)]
        fail_delta: f64,
        /// Fail when foreign paint exceeds this share of the foreground (0..=1)
        #[arg(long)]
        max_foreign: Option<f64>,
        /// Treat drift warnings as failures
        #[arg(long)]
        strict: bool,
        /// Background handling: auto, none, or a hex color
        #[arg(long, default_value = "auto", value_parser = BgMode::parse)]
        bg: BgMode,
        /// Machine-readable JSON on stdout
        #[arg(long)]
        json: bool,
        /// Disable ANSI colors
        #[arg(long)]
        no_color: bool,
    },
    /// Compare the palettes of two images directly
    Diff {
        /// Baseline image
        a: PathBuf,
        /// Image to compare against the baseline
        b: PathBuf,
        /// Number of palette colors to extract from each side
        #[arg(short = 'k', long, default_value_t = 8)]
        colors: usize,
        /// Background handling: auto, none, or a hex color
        #[arg(long, default_value = "auto", value_parser = BgMode::parse)]
        bg: BgMode,
        /// Machine-readable JSON on stdout
        #[arg(long)]
        json: bool,
        /// Disable ANSI colors
        #[arg(long)]
        no_color: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("palettefit: error: {err:#}");
            ExitCode::from(2)
        }
    }
}

fn style(no_color: bool) -> Style {
    let choice = if no_color {
        ColorChoice::Never
    } else {
        ColorChoice::Auto
    };
    Style::new(choice, std::io::stdout().is_terminal())
}

fn extract_clusters(sampled: &SampledImage, k: usize) -> Vec<Cluster> {
    median_cut(sampled.entries.clone(), k)
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Extract {
            image,
            colors,
            bg,
            name,
            out,
            json,
            no_color,
        } => {
            let sampled = sample(&image, bg)?;
            let clusters = extract_clusters(&sampled, colors);
            anyhow::ensure!(
                !clusters.is_empty(),
                "no foreground pixels left after background exclusion in {}",
                image.display()
            );
            let source = image.file_name().map(|n| n.to_string_lossy().into_owned());
            let palette = Palette::from_clusters(&clusters, name.as_deref(), source.as_deref());

            if json {
                println!("{}", extract_json(&image, &sampled, &clusters));
            } else if let Some(out_path) = &out {
                std::fs::write(out_path, palette.to_text())?;
                let st = style(no_color);
                eprintln!(
                    "wrote {} colors to {} (background: {})",
                    clusters.len(),
                    out_path.display(),
                    bg_desc(&sampled)
                );
                eprint!("{}", st.palette_table(&clusters));
            } else {
                print!("{}", palette.to_text());
                let st = style(no_color);
                eprintln!("background: {}", bg_desc(&sampled));
                eprint!("{}", st.palette_table(&clusters));
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Check {
            images,
            palette,
            warn_delta,
            fail_delta,
            max_foreign,
            strict,
            bg,
            json,
            no_color,
        } => {
            anyhow::ensure!(
                warn_delta <= fail_delta,
                "--warn-delta must not exceed --fail-delta"
            );
            let palette = Palette::load(&palette)?;
            let thresholds = Thresholds {
                warn_delta,
                fail_delta,
                max_foreign,
                strict,
            };
            let k = check::extract_k(palette.colors.len());
            let st = style(no_color);

            let mut any_fail = false;
            let mut json_images = Vec::new();
            for image in &images {
                let sampled = sample(image, bg)?;
                let clusters = extract_clusters(&sampled, k);
                let report = check::check(&palette, &clusters, &thresholds);
                if report.verdict == check::Verdict::Fail {
                    any_fail = true;
                }
                if json {
                    json_images.push(check_image_json(image, &sampled, &report));
                } else {
                    print!("{}", st.check_table(&image.display().to_string(), &report));
                }
            }
            if json {
                println!(
                    "{}",
                    check_json(&palette, &thresholds, &json_images, any_fail)
                );
            }
            Ok(if any_fail {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            })
        }
        Command::Diff {
            a,
            b,
            colors,
            bg,
            json,
            no_color,
        } => {
            let sa = sample(&a, bg)?;
            let sb = sample(&b, bg)?;
            let ca = extract_clusters(&sa, colors);
            let cb = extract_clusters(&sb, colors);
            anyhow::ensure!(
                !ca.is_empty() && !cb.is_empty(),
                "one of the images has no foreground pixels"
            );

            let mut rows = Vec::new();
            let mut weighted = 0.0;
            let mut weight_sum = 0.0;
            let mut max_delta: f64 = 0.0;
            for c in &ca {
                let (nearest, delta) = cb
                    .iter()
                    .map(|o| (o.color, delta_e_rgb(c.color, o.color)))
                    .min_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal))
                    .expect("cb is non-empty");
                weighted += delta * c.weight;
                weight_sum += c.weight;
                max_delta = max_delta.max(delta);
                rows.push((c.color, c.weight, nearest, delta));
            }
            let mean = weighted / weight_sum;

            if json {
                let mut items = String::new();
                for (i, (color, weight, nearest, delta)) in rows.iter().enumerate() {
                    let _ = write!(
                        items,
                        "{}{{\"hex\": {}, \"weight\": {:.4}, \"nearest\": {}, \"delta_e\": {:.4}}}",
                        if i == 0 { "" } else { ", " },
                        json::quote(&color.hex()),
                        weight,
                        json::quote(&nearest.hex()),
                        delta
                    );
                }
                println!(
                    "{{\"tool\": \"palettefit\", \"version\": {}, \"command\": \"diff\", \
                     \"a\": {}, \"b\": {}, \"colors\": [{}], \
                     \"weighted_mean_delta_e\": {:.4}, \"max_delta_e\": {:.4}}}",
                    json::quote(VERSION),
                    json::quote(&a.display().to_string()),
                    json::quote(&b.display().to_string()),
                    items,
                    mean,
                    max_delta
                );
            } else {
                let st = style(no_color);
                println!("{} vs {}", a.display(), b.display());
                for (color, weight, nearest, delta) in &rows {
                    println!(
                        "  {}#{}  {:>5.1}%  ->  {}#{}  dE {:>5.2}",
                        st.swatch(*color),
                        color.hex(),
                        weight * 100.0,
                        st.swatch(*nearest),
                        nearest.hex(),
                        delta
                    );
                }
                println!("weighted mean dE {mean:.2}, max dE {max_delta:.2}");
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn bg_desc(sampled: &SampledImage) -> String {
    match sampled.background {
        Some(c) => format!(
            "#{} excluded ({:.1}% of samples dropped)",
            c.hex(),
            sampled.excluded_share * 100.0
        ),
        None => "none excluded".to_string(),
    }
}

fn color_items(clusters: &[Cluster]) -> String {
    let mut out = String::new();
    for (i, c) in clusters.iter().enumerate() {
        let _ = write!(
            out,
            "{}{{\"hex\": {}, \"weight\": {:.4}}}",
            if i == 0 { "" } else { ", " },
            json::quote(&c.color.hex()),
            c.weight
        );
    }
    out
}

fn extract_json(image: &std::path::Path, sampled: &SampledImage, clusters: &[Cluster]) -> String {
    format!(
        "{{\"tool\": \"palettefit\", \"version\": {}, \"command\": \"extract\", \
         \"image\": {}, \"width\": {}, \"height\": {}, \"background\": {}, \
         \"excluded_share\": {:.4}, \"colors\": [{}]}}",
        json::quote(VERSION),
        json::quote(&image.display().to_string()),
        sampled.width,
        sampled.height,
        json::opt_str(sampled.background.map(|c| c.hex()).as_deref()),
        sampled.excluded_share,
        color_items(clusters)
    )
}

fn check_image_json(
    image: &std::path::Path,
    sampled: &SampledImage,
    report: &CheckReport,
) -> String {
    let mut colors = String::new();
    for (i, c) in report.colors.iter().enumerate() {
        let _ = write!(
            colors,
            "{}{{\"reference\": {}, \"label\": {}, \"nearest\": {}, \"delta_e\": {}, \"status\": {}}}",
            if i == 0 { "" } else { ", " },
            json::quote(&c.reference.hex()),
            json::opt_str(c.label.as_deref()),
            json::opt_str(c.nearest.map(|n| n.hex()).as_deref()),
            json::opt_f4(c.delta_e),
            json::quote(c.status.as_str())
        );
    }
    let mut foreign = String::new();
    for (i, f) in report.foreign.iter().enumerate() {
        let _ = write!(
            foreign,
            "{}{{\"hex\": {}, \"weight\": {:.4}, \"nearest_reference\": {}, \"delta_e\": {:.4}}}",
            if i == 0 { "" } else { ", " },
            json::quote(&f.color.hex()),
            f.weight,
            json::quote(&f.nearest_reference.hex()),
            f.delta_e
        );
    }
    format!(
        "{{\"image\": {}, \"verdict\": {}, \"background\": {}, \
         \"colors\": [{}], \"foreign\": [{}], \"foreign_share\": {:.4}}}",
        json::quote(&image.display().to_string()),
        json::quote(report.verdict.as_str()),
        json::opt_str(sampled.background.map(|c| c.hex()).as_deref()),
        colors,
        foreign,
        report.foreign_share
    )
}

fn check_json(palette: &Palette, t: &Thresholds, images: &[String], any_fail: bool) -> String {
    let mut pal_colors = String::new();
    for (i, c) in palette.colors.iter().enumerate() {
        let _ = write!(
            pal_colors,
            "{}{{\"hex\": {}, \"weight\": {}, \"label\": {}}}",
            if i == 0 { "" } else { ", " },
            json::quote(&c.color.hex()),
            json::opt_f4(c.weight),
            json::opt_str(c.label.as_deref())
        );
    }
    format!(
        "{{\"tool\": \"palettefit\", \"version\": {}, \"command\": \"check\", \
         \"palette\": {{\"name\": {}, \"source\": {}, \"colors\": [{}]}}, \
         \"thresholds\": {{\"warn_delta\": {}, \"fail_delta\": {}, \"max_foreign\": {}, \"strict\": {}}}, \
         \"images\": [{}], \"verdict\": {}}}",
        json::quote(VERSION),
        json::opt_str(palette.name.as_deref()),
        json::opt_str(palette.source.as_deref()),
        pal_colors,
        t.warn_delta,
        t.fail_delta,
        json::opt_f4(t.max_foreign),
        t.strict,
        images.join(", "),
        json::quote(if any_fail { "fail" } else { "pass" })
    )
}
