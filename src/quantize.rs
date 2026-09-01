//! Deterministic palette extraction via median-cut.
//!
//! k-means with random seeding would give a different palette on every run,
//! which is useless for a CI tool. Median-cut over the color histogram is
//! fully deterministic: same image in, same palette out, on every platform.

use crate::color::Rgb8;
use std::collections::HashMap;

/// One extracted cluster: the weighted mean color of a median-cut box and the
/// fraction of counted pixels that fell into it.
#[derive(Debug, Clone, Copy)]
pub struct Cluster {
    pub color: Rgb8,
    pub weight: f64,
}

/// Histogram entry: a unique color and how many sampled pixels had it.
type Entry = (Rgb8, u64);

struct BoxRange {
    /// Indices into the shared entry vec (the vec is re-sorted per split, and
    /// each box owns a contiguous range).
    start: usize,
    end: usize,
    count: u64,
    widest_channel: usize,
    widest_range: u8,
}

fn channel(c: Rgb8, i: usize) -> u8 {
    match i {
        0 => c.r,
        1 => c.g,
        _ => c.b,
    }
}

fn measure(entries: &[Entry], start: usize, end: usize) -> BoxRange {
    let mut min = [255u8; 3];
    let mut max = [0u8; 3];
    let mut count = 0u64;
    for &(c, n) in &entries[start..end] {
        for i in 0..3 {
            let v = channel(c, i);
            min[i] = min[i].min(v);
            max[i] = max[i].max(v);
        }
        count += n;
    }
    let mut widest_channel = 0;
    let mut widest_range = 0u8;
    for i in 0..3 {
        let r = max[i] - min[i];
        if r > widest_range {
            widest_range = r;
            widest_channel = i;
        }
    }
    BoxRange {
        start,
        end,
        count,
        widest_channel,
        widest_range,
    }
}

/// Build a histogram from pixels. Deterministic: colors are totaled in a map,
/// then sorted, so input order does not matter.
pub fn histogram(pixels: impl IntoIterator<Item = Rgb8>) -> Vec<Entry> {
    let mut map: HashMap<Rgb8, u64> = HashMap::new();
    for p in pixels {
        *map.entry(p).or_insert(0) += 1;
    }
    let mut entries: Vec<Entry> = map.into_iter().collect();
    entries.sort_unstable();
    entries
}

/// Median-cut the histogram into at most `k` clusters, sorted by weight
/// descending (ties broken by hex ascending). Returns an empty vec when the
/// histogram is empty.
pub fn median_cut(mut entries: Vec<Entry>, k: usize) -> Vec<Cluster> {
    if entries.is_empty() || k == 0 {
        return Vec::new();
    }
    let total: u64 = entries.iter().map(|&(_, n)| n).sum();
    let len = entries.len();
    let mut boxes = vec![measure(&entries, 0, len)];

    while boxes.len() < k {
        // Split the box holding the most pixels among those that still span
        // more than one color value. Ties go to the lower start index, which
        // keeps the result deterministic.
        let candidate = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.widest_range > 0)
            .max_by(|(ia, a), (ib, b)| a.count.cmp(&b.count).then(ib.cmp(ia)))
            .map(|(i, _)| i);
        let Some(idx) = candidate else { break };
        let b = boxes.swap_remove(idx);

        let ch = b.widest_channel;
        entries[b.start..b.end].sort_unstable_by(|&(c1, _), &(c2, _)| {
            channel(c1, ch).cmp(&channel(c2, ch)).then(c1.cmp(&c2))
        });

        // Weighted median split: first index where the running count passes
        // half of the box, clamped so both halves are non-empty.
        let half = b.count / 2;
        let mut acc = 0u64;
        let mut cut = b.start + 1;
        for (i, &(_, n)) in entries[b.start..b.end].iter().enumerate() {
            acc += n;
            if acc > half {
                cut = b.start + i + 1;
                break;
            }
        }
        cut = cut.clamp(b.start + 1, b.end - 1);

        boxes.push(measure(&entries, b.start, cut));
        boxes.push(measure(&entries, cut, b.end));
    }

    let mut clusters: Vec<Cluster> = boxes
        .iter()
        .map(|b| {
            let mut sum = [0u64; 3];
            for &(c, n) in &entries[b.start..b.end] {
                sum[0] += u64::from(c.r) * n;
                sum[1] += u64::from(c.g) * n;
                sum[2] += u64::from(c.b) * n;
            }
            let mean = |s: u64| (s as f64 / b.count as f64).round() as u8;
            Cluster {
                color: Rgb8::new(mean(sum[0]), mean(sum[1]), mean(sum[2])),
                weight: b.count as f64 / total as f64,
            }
        })
        .collect();
    clusters.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.color.cmp(&b.color))
    });
    clusters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_solid_colors_come_back_exactly() {
        let red = Rgb8::new(230, 57, 70);
        let blue = Rgb8::new(69, 123, 157);
        let cream = Rgb8::new(241, 250, 238);
        let pixels: Vec<Rgb8> = std::iter::repeat_n(red, 500)
            .chain(std::iter::repeat_n(blue, 300))
            .chain(std::iter::repeat_n(cream, 200))
            .collect();
        let clusters = median_cut(histogram(pixels), 3);
        assert_eq!(clusters.len(), 3);
        assert_eq!(clusters[0].color, red);
        assert_eq!(clusters[1].color, blue);
        assert_eq!(clusters[2].color, cream);
        assert!((clusters[0].weight - 0.5).abs() < 1e-9);
    }

    #[test]
    fn fewer_colors_than_k_does_not_pad() {
        let clusters = median_cut(histogram(vec![Rgb8::new(1, 2, 3); 10]), 8);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].color, Rgb8::new(1, 2, 3));
        assert!((clusters[0].weight - 1.0).abs() < 1e-9);
    }

    #[test]
    fn deterministic_regardless_of_input_order() {
        let mut a = Vec::new();
        for i in 0..64u8 {
            for _ in 0..=i {
                a.push(Rgb8::new(i * 4, 255 - i * 2, i));
            }
        }
        let mut b = a.clone();
        b.reverse();
        let ca = median_cut(histogram(a), 5);
        let cb = median_cut(histogram(b), 5);
        let key = |cs: &[Cluster]| -> Vec<(String, u64)> {
            cs.iter()
                .map(|c| (c.color.hex(), (c.weight * 1e12) as u64))
                .collect()
        };
        assert_eq!(key(&ca), key(&cb));
    }

    #[test]
    fn empty_input_yields_empty_palette() {
        assert!(median_cut(histogram(Vec::<Rgb8>::new()), 4).is_empty());
    }
}
