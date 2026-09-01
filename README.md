# palettefit

Audit character art and brand assets for color drift.

Extract a reference palette from a model sheet once, then score any number of
other images against it -- new episode pages, AI-generated cuts, game sprites,
picture-book spreads, blog thumbnails. palettefit tells you which reference
colors are held, which have drifted, which are gone, and how much paint in the
image belongs to no reference color at all.

Deterministic, offline, single binary. Same input, same output, every time --
so it works as a CI gate.

## Why

Keeping a character on-model across dozens of images is a real production
problem, and it got bigger now that AI image generators are part of many
workflows: even with a locked reference image, generated cuts drift in hue and
saturation from shot to shot. Reviewing that by eye does not scale, and eyes
adapt -- a slow hue shift across twenty pages is exactly what humans miss.

palettefit measures the drift instead, with CIEDE2000 (the CIE's current
color-difference formula), and turns "does the jacket still have the right
blue?" into an exit code.

## Install

```sh
cargo install --git https://github.com/printemps-tokyo/palettefit
```

Or grab a binary from Releases. Reads PNG, JPEG and WebP.

## Usage

### 1. Extract a reference palette from the model sheet

```sh
palettefit extract miko-ref-sheet.png -k 6 --name Miko -o miko.palette
```

```
# palettefit palette v1
# name: Miko
# source: miko-ref-sheet.png
e63946 0.4123
457b9d 0.2881
f1faee 0.1832
1d3557 0.0914
...
```

The palette file is plain text: hex, weight (share of the reference's
foreground pixels), and an optional label you add by hand. Labeling is worth
the minute it takes -- reports then say `jacket` instead of `457b9d`:

```
e63946 0.4123 hair
457b9d 0.2881 jacket
f1faee 0.1832 skin
1d3557 0.0914 eyes
```

You can also write a palette from scratch (weights optional) if your character
already has official colors.

### 2. Check images against it

```sh
palettefit check episode2/*.png --palette miko.palette
```

```
p01.png: PASS
  ok      #457b9d  nearest #457b9d  dE  0.00  jacket
  ok      #e63946  nearest #e63946  dE  0.00  hair
  ok      #f1faee  nearest #f1faee  dE  0.00  skin
  ok      #1d3557  nearest #1d3557  dE  0.00  eyes
p07.png: FAIL
  missing #457b9d  nearest #24406b  dE 21.29  jacket
  ok      #e63946  nearest #e53a48  dE  0.52  hair
  ok      #f1faee  nearest #f0f9ec  dE  0.46  skin
  drift   #1d3557  nearest #24406b  dE  4.16  eyes
  foreign paint: 48.2% of foreground
    #3ca06c   48.2%  (nearest reference #f1faee at dE 32.93)
```

(On a terminal each hex value is preceded by a swatch painted in that color.)

Exit code 0 when every image passes (warnings allowed), 1 when any image
fails, 2 on errors. `--strict` turns drift warnings into failures.

### 3. Or compare two images directly

```sh
palettefit diff approved-cover.png revised-cover.png
```

Reports each dominant color of the baseline, its nearest match in the other
image, and the weighted mean/max CIEDE2000 distance.

## How it works

1. Pixels are sampled on a fixed grid (at most ~262k samples), transparent
   pixels are dropped, and the background is excluded -- by corner detection
   (`--bg auto`, default), a declared color (`--bg '#ffffff'`), or not at all
   (`--bg none`).
2. The remaining foreground is quantized with median-cut. Median-cut is used
   instead of k-means precisely because it has no random seeding: the palette
   is identical on every run and platform.
3. Each reference color is matched to its nearest extracted cluster in CIELAB
   space using CIEDE2000, implemented after Sharma, Wu and Dalal (2005) and
   validated against all 34 of their published test pairs.
4. Clusters far from every reference color are reported as foreign paint.

## Thresholds

| status | meaning | default |
| --- | --- | --- |
| `ok` | nearest cluster within `--warn-delta` | dE <= 3 |
| `drift` | present but shifted; warns (fails with `--strict`) | dE <= 8 |
| `missing` | nothing anywhere near; fails | dE > 8 |

As a rule of thumb, a CIEDE2000 difference near 1 is at the edge of what a
viewer notices. The defaults are deliberately looser than that, because
palette extraction itself shifts cluster means slightly (anti-aliasing,
shading, JPEG artifacts). Tighten them per project:

```sh
palettefit check pages/*.png -p miko.palette --warn-delta 2 --fail-delta 5
palettefit check pages/*.png -p miko.palette --max-foreign 0.25
```

`--max-foreign 0.25` additionally fails an image when more than 25% of its
foreground belongs to no reference color -- useful for sprites and flat-color
art, too noisy for painterly full scenes.

## JSON output

Every subcommand takes `--json` and prints a single JSON document on stdout:

```sh
palettefit check pages/*.png -p miko.palette --json | jq '.images[].verdict'
```

The check document carries the palette, the thresholds used, and per image:
per-color `status`/`delta_e`/`nearest`, the `foreign` list, `foreign_share`,
and a `verdict`. The extract document carries the detected `background`,
`excluded_share` and the extracted `colors` with weights.

## CI example

```yaml
- name: Character colors stay on-model
  run: palettefit check art/exports/*.png --palette art/miko.palette --strict
```

## What it does not do

- It audits palette-level color identity, not placement. A jacket-colored
  hat passes; that review still needs eyes. Shape, anatomy and line quality
  are out of scope.
- Whole-image color grading (a warm evening scene) legitimately shifts every
  color; check graded shots against a palette extracted from a graded
  reference, or lean on `diff` instead.
- Colors are compared in sRGB-derived CIELAB (D65). Files are decoded as
  sRGB; embedded ICC profiles are not applied.
- Heavily anti-aliased or very small art yields mixed-edge clusters; prefer
  reasonably sized flats for reference sheets.

## License

MIT
