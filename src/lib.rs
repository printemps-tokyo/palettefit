//! palettefit: audit character art and brand assets for color drift.
//!
//! Extract a reference palette from a model sheet, then score any number of
//! other images (new episode pages, AI-generated cuts, sprites, thumbnails)
//! against it with the CIEDE2000 color-difference formula. Deterministic,
//! offline, CI-friendly.

pub mod check;
pub mod color;
pub mod input;
pub mod json;
pub mod palette;
pub mod quantize;
pub mod report;
