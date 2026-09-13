# Contributing

Thanks for your interest in contributing to palettefit.

## Development

1. Fork / clone the repository
2. Install a stable Rust toolchain (https://rustup.rs)
3. Create a branch: `git switch -c feat/your-change`
4. Make your change and verify locally:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --all-targets -- -D warnings
   cargo test --all
   cargo build --release
   ```
5. Commit and open a pull request

## Ground rules

- Same input, same output, on every machine. Palette extraction is median-cut
  rather than k-means precisely because there is no seed to differ over, and
  sampling walks a fixed grid. A change that introduces randomness, thread
  order dependence or floating-point drift across platforms breaks the reason
  this tool can sit in CI.
- The colour maths has a reference. CIEDE2000 is implemented after Sharma, Wu
  and Dalal (2005) and pinned by all 34 of their published test pairs in
  `src/color.rs`. Any change there has to keep those passing; a new colour
  formula needs the paper it comes from and its own test vectors.
- Thresholds are policy, not physics. `--warn-delta` and `--fail-delta` are
  heuristics, deliberately looser than the perceptual limit because extraction
  itself shifts cluster means. Keep them flags with documented defaults, and
  keep the README honest about what they do and do not mean.
- Report evidence before advice. A finding names the reference colour, the
  nearest cluster it found, and the distance between them. No score out of a
  hundred, and no claim about what the artist intended.
- Nothing leaves the machine. No network calls and no telemetry; images are
  decoded locally and only the three formats character art actually ships in
  are compiled in.
- The palette file stays hand-editable plain text. It is a format people write
  by hand when a character already has official colours, so keep it readable
  and keep the parser forgiving about comments and whitespace.

## Tests

Unit tests live next to the code they cover (colour conversion, CIEDE2000,
median-cut, palette parsing, background detection). End-to-end tests in
`tests/cli.rs` build synthetic images in code and run the built binary against
them, so there are no binary fixtures to keep in sync. Add a test that breaks
exactly one thing about an otherwise clean image.

## Reporting bugs

Open an issue with the images involved if you can share them, the palette file,
the command line, and what you expected. When the images are not shareable, the
`--json` output of `extract` on both sides is usually enough to reason about a
mismatch.
