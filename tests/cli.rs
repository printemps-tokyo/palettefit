//! End-to-end tests: run the built binary against synthetic images.

use image::{Rgba, RgbaImage};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_palettefit");

const RED: [u8; 4] = [230, 57, 70, 255];
const BLUE: [u8; 4] = [69, 123, 157, 255];
const NAVY: [u8; 4] = [29, 53, 87, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// A "character": red hair block, blue jacket block, navy eyes stripe on a
/// white background.
fn character(jacket: [u8; 4]) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(100, 100, Rgba(WHITE));
    for y in 10..40 {
        for x in 20..80 {
            img.put_pixel(x, y, Rgba(RED));
        }
    }
    for y in 45..90 {
        for x in 25..75 {
            img.put_pixel(x, y, Rgba(jacket));
        }
    }
    for y in 41..44 {
        for x in 40..60 {
            img.put_pixel(x, y, Rgba(NAVY));
        }
    }
    img
}

fn save(dir: &Path, name: &str, img: &RgbaImage) -> PathBuf {
    let p = dir.join(name);
    img.save(&p).unwrap();
    p
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn extract_then_check_same_image_passes() {
    let dir = tempfile::tempdir().unwrap();
    let ref_img = save(dir.path(), "ref.png", &character(BLUE));
    let pal = dir.path().join("pal.txt");

    let o = run(&[
        "extract",
        ref_img.to_str().unwrap(),
        "-k",
        "3",
        "-o",
        pal.to_str().unwrap(),
        "--no-color",
    ]);
    assert!(o.status.success(), "extract failed: {o:?}");
    let text = std::fs::read_to_string(&pal).unwrap();
    assert!(text.contains("e63946"), "palette missing red: {text}");
    assert!(text.contains("457b9d"), "palette missing blue: {text}");
    // White background must have been excluded.
    assert!(!text.contains("ffffff"), "background leaked in: {text}");

    let o = run(&[
        "check",
        ref_img.to_str().unwrap(),
        "--palette",
        pal.to_str().unwrap(),
        "--no-color",
    ]);
    assert!(o.status.success(), "check failed: {}", stdout(&o));
    assert!(stdout(&o).contains("PASS"));
}

#[test]
fn recolored_jacket_fails_with_exit_1() {
    let dir = tempfile::tempdir().unwrap();
    let ref_img = save(dir.path(), "ref.png", &character(BLUE));
    let bad_img = save(dir.path(), "bad.png", &character([60, 160, 60, 255]));
    let pal = dir.path().join("pal.txt");
    run(&[
        "extract",
        ref_img.to_str().unwrap(),
        "-k",
        "3",
        "-o",
        pal.to_str().unwrap(),
    ]);

    let o = run(&[
        "check",
        bad_img.to_str().unwrap(),
        "--palette",
        pal.to_str().unwrap(),
        "--no-color",
    ]);
    assert_eq!(o.status.code(), Some(1), "expected exit 1: {}", stdout(&o));
    let out = stdout(&o);
    assert!(out.contains("FAIL"), "no FAIL in: {out}");
    assert!(out.contains("missing"), "no missing color in: {out}");
    assert!(out.contains("foreign paint"), "no foreign report in: {out}");
}

#[test]
fn check_json_is_emitted_and_names_the_drifted_color() {
    let dir = tempfile::tempdir().unwrap();
    let ref_img = save(dir.path(), "ref.png", &character(BLUE));
    let bad_img = save(dir.path(), "bad.png", &character([60, 160, 60, 255]));
    let pal = dir.path().join("pal.txt");
    run(&[
        "extract",
        ref_img.to_str().unwrap(),
        "-k",
        "3",
        "-o",
        pal.to_str().unwrap(),
    ]);

    let o = run(&[
        "check",
        bad_img.to_str().unwrap(),
        "--palette",
        pal.to_str().unwrap(),
        "--json",
    ]);
    let out = stdout(&o);
    assert!(out.contains("\"command\": \"check\""), "{out}");
    assert!(out.contains("\"verdict\": \"fail\""), "{out}");
    assert!(out.contains("\"status\": \"missing\""), "{out}");
    assert!(out.contains("\"reference\": \"457b9d\""), "{out}");
}

#[test]
fn diff_reports_zero_for_identical_images() {
    let dir = tempfile::tempdir().unwrap();
    let a = save(dir.path(), "a.png", &character(BLUE));
    let b = save(dir.path(), "b.png", &character(BLUE));
    let o = run(&["diff", a.to_str().unwrap(), b.to_str().unwrap(), "--json"]);
    assert!(o.status.success());
    let out = stdout(&o);
    assert!(
        out.contains("\"weighted_mean_delta_e\": 0.0000"),
        "expected zero mean delta: {out}"
    );
}

#[test]
fn missing_file_exits_2() {
    let o = run(&["extract", "no-such-file.png"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("error"));
}

#[test]
fn transparent_background_needs_no_bg_flag() {
    let dir = tempfile::tempdir().unwrap();
    let mut img = RgbaImage::from_pixel(50, 50, Rgba([0, 0, 0, 0]));
    for y in 10..40 {
        for x in 10..40 {
            img.put_pixel(x, y, Rgba(RED));
        }
    }
    let p = save(dir.path(), "t.png", &img);
    let o = run(&["extract", p.to_str().unwrap(), "--bg", "none", "--json"]);
    assert!(o.status.success());
    let out = stdout(&o);
    assert!(out.contains("\"e63946\""), "{out}");
    assert!(
        !out.contains("\"000000\""),
        "transparent pixels leaked: {out}"
    );
}

#[test]
fn contrast_checks_pairs_and_export_writes_both_formats() {
    let dir = tempfile::tempdir().unwrap();
    let pal = dir.path().join("p.palette");
    std::fs::write(
        &pal,
        "# name: Test\nffffff 0.5 paper\n1d3557 0.3 ink\n777777 0.2 grey\n",
    )
    .unwrap();
    let pal_s = pal.to_str().unwrap();

    let all = Command::new(BIN)
        .args(["contrast", pal_s, "--no-color"])
        .output()
        .unwrap();
    assert_eq!(all.status.code(), Some(0), "listing never fails");
    assert_eq!(String::from_utf8_lossy(&all.stdout).lines().count(), 4);

    let ok = Command::new(BIN)
        .args(["contrast", pal_s, "--pairs", "ink:paper", "--level", "aaa"])
        .output()
        .unwrap();
    assert_eq!(ok.status.code(), Some(0));
    let bad = Command::new(BIN)
        .args([
            "contrast",
            pal_s,
            "--pairs",
            "ink:paper,grey:paper",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        bad.status.code(),
        Some(1),
        "#777777 on white is below 4.5:1"
    );
    let doc = String::from_utf8_lossy(&bad.stdout);
    assert!(doc.contains("\"standard\": \"WCAG 2.2\"") && doc.contains("\"pass\": false"));

    let gpl = dir.path().join("p.gpl");
    let ase = dir.path().join("p.ase");
    for out in [&gpl, &ase] {
        let o = Command::new(BIN)
            .args(["export", pal_s, "-o", out.to_str().unwrap()])
            .output()
            .unwrap();
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
    assert!(std::fs::read_to_string(&gpl)
        .unwrap()
        .starts_with("GIMP Palette\nName: Test\n"));
    assert_eq!(&std::fs::read(&ase).unwrap()[0..4], b"ASEF");
    let unknown = Command::new(BIN)
        .args([
            "export",
            pal_s,
            "-o",
            dir.path().join("p.txt").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(unknown.status.code(), Some(2));
}
