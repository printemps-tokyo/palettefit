//! sRGB to CIELAB conversion and the CIEDE2000 color-difference formula.
//!
//! The CIEDE2000 implementation follows Sharma, Wu and Dalal, "The CIEDE2000
//! Color-Difference Formula: Implementation Notes, Supplementary Test Data,
//! and Mathematical Observations" (Color Research & Application, 2005) and is
//! validated in the unit tests against all 34 published test pairs.

/// An 8-bit sRGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Lowercase `rrggbb` hex without the leading `#`.
    pub fn hex(&self) -> String {
        format!("{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Parse `rrggbb` or `#rrggbb` (case-insensitive). Short `rgb` form is
    /// accepted too because hand-written palette files tend to use it.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.strip_prefix('#').unwrap_or(s);
        let full = match s.len() {
            6 => s.to_string(),
            3 => s.chars().flat_map(|c| [c, c]).collect(),
            _ => return None,
        };
        if !full.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let v = u32::from_str_radix(&full, 16).ok()?;
        Some(Self::new((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }

    pub fn to_lab(self) -> Lab {
        srgb_to_lab(self)
    }
}

/// A color in CIELAB (D65 reference white).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lab {
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

fn srgb_channel_to_linear(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB (IEC 61966-2-1) -> XYZ (D65) -> CIELAB.
pub fn srgb_to_lab(rgb: Rgb8) -> Lab {
    let r = srgb_channel_to_linear(rgb.r);
    let g = srgb_channel_to_linear(rgb.g);
    let b = srgb_channel_to_linear(rgb.b);

    // sRGB linear RGB -> XYZ, D65 white.
    let x = 0.4124564 * r + 0.3575761 * g + 0.1804375 * b;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = 0.0193339 * r + 0.1191920 * g + 0.9503041 * b;

    // D65 reference white.
    const XN: f64 = 0.95047;
    const YN: f64 = 1.0;
    const ZN: f64 = 1.08883;

    fn f(t: f64) -> f64 {
        const DELTA: f64 = 6.0 / 29.0;
        if t > DELTA * DELTA * DELTA {
            t.cbrt()
        } else {
            t / (3.0 * DELTA * DELTA) + 4.0 / 29.0
        }
    }

    let fx = f(x / XN);
    let fy = f(y / YN);
    let fz = f(z / ZN);

    Lab {
        l: 116.0 * fy - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
    }
}

/// CIEDE2000 color difference between two Lab colors (kL = kC = kH = 1).
pub fn ciede2000(c1: Lab, c2: Lab) -> f64 {
    const POW25_7: f64 = 6103515625.0; // 25^7

    let cab1 = c1.a.hypot(c1.b);
    let cab2 = c2.a.hypot(c2.b);
    let cab_mean = (cab1 + cab2) / 2.0;

    let g = 0.5 * (1.0 - (cab_mean.powi(7) / (cab_mean.powi(7) + POW25_7)).sqrt());
    let ap1 = (1.0 + g) * c1.a;
    let ap2 = (1.0 + g) * c2.a;
    let cp1 = ap1.hypot(c1.b);
    let cp2 = ap2.hypot(c2.b);

    // Hue angles in degrees, normalized to [0, 360).
    let hp = |b: f64, ap: f64, cp: f64| -> f64 {
        if cp == 0.0 {
            0.0
        } else {
            let h = b.atan2(ap).to_degrees();
            if h < 0.0 {
                h + 360.0
            } else {
                h
            }
        }
    };
    let hp1 = hp(c1.b, ap1, cp1);
    let hp2 = hp(c2.b, ap2, cp2);

    let dl = c2.l - c1.l;
    let dc = cp2 - cp1;

    let dhp = if cp1 * cp2 == 0.0 {
        0.0
    } else {
        let d = hp2 - hp1;
        if d.abs() <= 180.0 {
            d
        } else if d > 180.0 {
            d - 360.0
        } else {
            d + 360.0
        }
    };
    let dh = 2.0 * (cp1 * cp2).sqrt() * (dhp / 2.0).to_radians().sin();

    let l_mean = (c1.l + c2.l) / 2.0;
    let cp_mean = (cp1 + cp2) / 2.0;

    let hp_mean = if cp1 * cp2 == 0.0 {
        hp1 + hp2
    } else {
        let sum = hp1 + hp2;
        if (hp1 - hp2).abs() <= 180.0 {
            sum / 2.0
        } else if sum < 360.0 {
            (sum + 360.0) / 2.0
        } else {
            (sum - 360.0) / 2.0
        }
    };

    let t = 1.0 - 0.17 * (hp_mean - 30.0).to_radians().cos()
        + 0.24 * (2.0 * hp_mean).to_radians().cos()
        + 0.32 * (3.0 * hp_mean + 6.0).to_radians().cos()
        - 0.20 * (4.0 * hp_mean - 63.0).to_radians().cos();

    let dtheta = 30.0 * (-((hp_mean - 275.0) / 25.0).powi(2)).exp();
    let rc = 2.0 * (cp_mean.powi(7) / (cp_mean.powi(7) + POW25_7)).sqrt();
    let l50 = (l_mean - 50.0).powi(2);
    let sl = 1.0 + 0.015 * l50 / (20.0 + l50).sqrt();
    let sc = 1.0 + 0.045 * cp_mean;
    let sh = 1.0 + 0.015 * cp_mean * t;
    let rt = -(2.0 * dtheta).to_radians().sin() * rc;

    let dl_s = dl / sl;
    let dc_s = dc / sc;
    let dh_s = dh / sh;

    (dl_s * dl_s + dc_s * dc_s + dh_s * dh_s + rt * dc_s * dh_s).sqrt()
}

/// CIEDE2000 between two sRGB colors.
pub fn delta_e_rgb(c1: Rgb8, c2: Rgb8) -> f64 {
    ciede2000(c1.to_lab(), c2.to_lab())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip_and_short_form() {
        let c = Rgb8::parse("#E63946").unwrap();
        assert_eq!(c, Rgb8::new(0xe6, 0x39, 0x46));
        assert_eq!(c.hex(), "e63946");
        assert_eq!(Rgb8::parse("fff").unwrap(), Rgb8::new(255, 255, 255));
        assert!(Rgb8::parse("12345").is_none());
        assert!(Rgb8::parse("gggggg").is_none());
    }

    #[test]
    fn lab_of_white_and_black() {
        let w = srgb_to_lab(Rgb8::new(255, 255, 255));
        assert!((w.l - 100.0).abs() < 0.01, "white L = {}", w.l);
        assert!(w.a.abs() < 0.01 && w.b.abs() < 0.01);
        let k = srgb_to_lab(Rgb8::new(0, 0, 0));
        assert!(k.l.abs() < 0.01);
    }

    #[test]
    fn identical_colors_have_zero_difference() {
        let c = srgb_to_lab(Rgb8::new(120, 33, 200));
        assert_eq!(ciede2000(c, c), 0.0);
    }

    /// All 34 test pairs from Sharma, Wu and Dalal (2005), Table 1.
    #[test]
    fn sharma_2005_test_pairs() {
        #[rustfmt::skip]
        const CASES: [(f64, f64, f64, f64, f64, f64, f64); 34] = [
            (50.0000, 2.6772, -79.7751, 50.0000, 0.0000, -82.7485, 2.0425),
            (50.0000, 3.1571, -77.2803, 50.0000, 0.0000, -82.7485, 2.8615),
            (50.0000, 2.8361, -74.0200, 50.0000, 0.0000, -82.7485, 3.4412),
            (50.0000, -1.3802, -84.2814, 50.0000, 0.0000, -82.7485, 1.0000),
            (50.0000, -1.1848, -84.8006, 50.0000, 0.0000, -82.7485, 1.0000),
            (50.0000, -0.9009, -85.5211, 50.0000, 0.0000, -82.7485, 1.0000),
            (50.0000, 0.0000, 0.0000, 50.0000, -1.0000, 2.0000, 2.3669),
            (50.0000, -1.0000, 2.0000, 50.0000, 0.0000, 0.0000, 2.3669),
            (50.0000, 2.4900, -0.0010, 50.0000, -2.4900, 0.0009, 7.1792),
            (50.0000, 2.4900, -0.0010, 50.0000, -2.4900, 0.0010, 7.1792),
            (50.0000, 2.4900, -0.0010, 50.0000, -2.4900, 0.0011, 7.2195),
            (50.0000, 2.4900, -0.0010, 50.0000, -2.4900, 0.0012, 7.2195),
            (50.0000, -0.0010, 2.4900, 50.0000, 0.0009, -2.4900, 4.8045),
            (50.0000, -0.0010, 2.4900, 50.0000, 0.0010, -2.4900, 4.8045),
            (50.0000, -0.0010, 2.4900, 50.0000, 0.0011, -2.4900, 4.7461),
            (50.0000, 2.5000, 0.0000, 50.0000, 0.0000, -2.5000, 4.3065),
            (50.0000, 2.5000, 0.0000, 73.0000, 25.0000, -18.0000, 27.1492),
            (50.0000, 2.5000, 0.0000, 61.0000, -5.0000, 29.0000, 22.8977),
            (50.0000, 2.5000, 0.0000, 56.0000, -27.0000, -3.0000, 31.9030),
            (50.0000, 2.5000, 0.0000, 58.0000, 24.0000, 15.0000, 19.4535),
            (50.0000, 2.5000, 0.0000, 50.0000, 3.1736, 0.5854, 1.0000),
            (50.0000, 2.5000, 0.0000, 50.0000, 3.2972, 0.0000, 1.0000),
            (50.0000, 2.5000, 0.0000, 50.0000, 1.8634, 0.5757, 1.0000),
            (50.0000, 2.5000, 0.0000, 50.0000, 3.2592, 0.3350, 1.0000),
            (60.2574, -34.0099, 36.2677, 60.4626, -34.1751, 39.4387, 1.2644),
            (63.0109, -31.0961, -5.8663, 62.8187, -29.7946, -4.0864, 1.2630),
            (61.2901, 3.7196, -5.3901, 61.4292, 2.2480, -4.9620, 1.8731),
            (35.0831, -44.1164, 3.7933, 35.0232, -40.0716, 1.5901, 1.8645),
            (22.7233, 20.0904, -46.6940, 23.0331, 14.9730, -42.5619, 2.0373),
            (36.4612, 47.8580, 18.3852, 36.2715, 50.5065, 21.2231, 1.4146),
            (90.8027, -2.0831, 1.4410, 91.1528, -1.6435, 0.0447, 1.4441),
            (90.9257, -0.5406, -0.9208, 88.6381, -0.8985, -0.7239, 1.5381),
            (6.7747, -0.2908, -2.4247, 5.8714, -0.0985, -2.2286, 0.6377),
            (2.0776, 0.0795, -1.1350, 0.9033, -0.0636, -0.5514, 0.9082),
        ];
        for (i, &(l1, a1, b1, l2, a2, b2, expected)) in CASES.iter().enumerate() {
            let got = ciede2000(
                Lab {
                    l: l1,
                    a: a1,
                    b: b1,
                },
                Lab {
                    l: l2,
                    a: a2,
                    b: b2,
                },
            );
            assert!(
                (got - expected).abs() < 1e-4,
                "pair {}: got {got:.4}, expected {expected:.4}",
                i + 1
            );
            // The formula is symmetric.
            let rev = ciede2000(
                Lab {
                    l: l2,
                    a: a2,
                    b: b2,
                },
                Lab {
                    l: l1,
                    a: a1,
                    b: b1,
                },
            );
            assert!((rev - expected).abs() < 1e-4, "pair {} reversed", i + 1);
        }
    }
}
