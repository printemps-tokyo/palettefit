//! Palette export for design tools: GIMP `.gpl` and Adobe `.ase`.
//!
//! Both layouts follow what GIMP's own loader reads (`app/core/
//! gimppalette-load.c`, GNOME/gimp at d1ac59a, read 2026-11-12):
//!
//! - GPL: a `GIMP Palette` line, optional `Name:` and `Columns:` lines,
//!   `#` comments, then one `R G B<TAB>name` line per color in 0..=255.
//! - ASE: big-endian. `ASEF`, version 1.0, a block count, then blocks of
//!   `type u16, length u32, body`. A group-start block (0xC001) carries the
//!   palette name; each color block (0x0001) holds a UTF-16BE name with its
//!   length in code units including the terminating NUL, the color model
//!   `"RGB "`, three float32 channels in 0..=1, and a color type (2 =
//!   normal); a group-end block (0xC002) closes the group.

use crate::palette::Palette;

/// The name a color is exported under: its label, else its hex value.
fn color_name(label: Option<&str>, hex: &str) -> String {
    label.map_or_else(|| format!("#{hex}"), str::to_string)
}

/// A GIMP palette (`.gpl`).
pub fn to_gpl(palette: &Palette) -> String {
    let mut out = String::from("GIMP Palette\n");
    let name = palette.name.as_deref().unwrap_or("palettefit");
    // Keep the header on one line whatever the name holds.
    out.push_str(&format!("Name: {}\n", name.replace(['\n', '\r'], " ")));
    out.push_str(&format!("Columns: {}\n", palette.colors.len().min(16)));
    out.push_str("# exported by palettefit\n");
    for c in &palette.colors {
        let name = color_name(c.label.as_deref(), &c.color.hex());
        out.push_str(&format!(
            "{:>3} {:>3} {:>3}\t{}\n",
            c.color.r,
            c.color.g,
            c.color.b,
            name.replace(['\n', '\r'], " ")
        ));
    }
    out
}

fn utf16_name(s: &str) -> Vec<u8> {
    let units: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let mut out = Vec::with_capacity(2 + units.len() * 2);
    out.extend_from_slice(&(units.len() as u16).to_be_bytes());
    for u in units {
        out.extend_from_slice(&u.to_be_bytes());
    }
    out
}

fn block(kind: u16, body: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(&kind.to_be_bytes());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
}

/// An Adobe Swatch Exchange file (`.ase`).
pub fn to_ase(palette: &Palette) -> Vec<u8> {
    let mut blocks = Vec::new();
    let name = palette.name.as_deref().unwrap_or("palettefit");
    block(0xC001, &utf16_name(name), &mut blocks);
    for c in &palette.colors {
        let mut body = utf16_name(&color_name(c.label.as_deref(), &c.color.hex()));
        body.extend_from_slice(b"RGB ");
        for v in [c.color.r, c.color.g, c.color.b] {
            body.extend_from_slice(&(f32::from(v) / 255.0).to_be_bytes());
        }
        body.extend_from_slice(&2u16.to_be_bytes()); // normal (not global, not spot)
        block(0x0001, &body, &mut blocks);
    }
    block(0xC002, &[], &mut blocks);

    let mut out = Vec::with_capacity(12 + blocks.len());
    out.extend_from_slice(b"ASEF");
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&((palette.colors.len() + 2) as u32).to_be_bytes());
    out.extend_from_slice(&blocks);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        Palette::parse("# name: Miko\ne63946 0.4 hair\n457b9d 0.3\n").unwrap()
    }

    #[test]
    fn gpl_layout() {
        let gpl = to_gpl(&palette());
        assert!(gpl.starts_with("GIMP Palette\nName: Miko\nColumns: 2\n"));
        assert!(gpl.contains("230  57  70\thair\n"));
        assert!(gpl.contains(" 69 123 157\t#457b9d\n"));
    }

    /// Walk the ASE the way GIMP's loader does and get back the colors.
    #[test]
    fn ase_reads_back() {
        let ase = to_ase(&palette());
        assert_eq!(&ase[0..4], b"ASEF");
        assert_eq!(ase[5], 0x01, "GIMP checks header[5] == 1");
        let u16_at = |i: usize| u16::from_be_bytes([ase[i], ase[i + 1]]);
        let u32_at = |i: usize| u32::from_be_bytes([ase[i], ase[i + 1], ase[i + 2], ase[i + 3]]);
        assert_eq!(u32_at(8), 4);
        let mut pos = 12;
        let mut kinds = Vec::new();
        let mut names = Vec::new();
        let mut colors = Vec::new();
        while pos < ase.len() {
            let kind = u16_at(pos);
            let len = u32_at(pos + 2) as usize;
            let body = &ase[pos + 6..pos + 6 + len];
            kinds.push(kind);
            if kind != 0xC002 {
                let n = u16::from_be_bytes([body[0], body[1]]) as usize;
                let units: Vec<u16> = (0..n)
                    .map(|i| u16::from_be_bytes([body[2 + 2 * i], body[3 + 2 * i]]))
                    .collect();
                assert_eq!(units.last(), Some(&0));
                names.push(String::from_utf16(&units[..n - 1]).unwrap());
                if kind == 1 {
                    let at = 2 + 2 * n;
                    assert_eq!(&body[at..at + 4], b"RGB ");
                    let f = |k: usize| {
                        f32::from_be_bytes(body[at + 4 + 4 * k..at + 8 + 4 * k].try_into().unwrap())
                    };
                    colors.push((
                        (f(0) * 255.0).round() as u8,
                        (f(1) * 255.0).round() as u8,
                        (f(2) * 255.0).round() as u8,
                    ));
                    assert_eq!(u16::from_be_bytes([body[at + 16], body[at + 17]]), 2);
                }
            }
            pos += 6 + len;
        }
        assert_eq!(pos, ase.len());
        assert_eq!(kinds, vec![0xC001, 1, 1, 0xC002]);
        assert_eq!(names, vec!["Miko", "hair", "#457b9d"]);
        assert_eq!(colors, vec![(0xe6, 0x39, 0x46), (0x45, 0x7b, 0x9d)]);
    }
}
