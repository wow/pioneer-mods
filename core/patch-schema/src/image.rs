//! Image edits in schema-v2 recipes: changes to a 16-bit RGB565 image stored in the decoded
//! application, described so that the recipe carries **no stock pixels**.
//!
//! An edit first erases a box: each row of the box is refilled by interpolating between the stock
//! pixels just left and right of it, which removes a label from a smooth background. It then draws
//! a glyph: an alpha mask the recipe's author drew (one hex digit of coverage per pixel), in the
//! colour of one stock pixel the recipe names by its position. Everything the edit takes from the
//! image is read from the owner's own file at patch time. The whole stock image is identified by
//! its SHA-256, the edit's precondition.
//!
//! The pixel arithmetic is fixed here so that every front end produces the same bytes:
//! [`ImageEdit::erase_row`] and [`blend`].

use serde::{Deserialize, Serialize};
use std::ops::Range;

/// Largest image an edit may name, in bytes (an 800x480 screen is 768,000).
pub const MAX_IMAGE_BYTES: u64 = 1024 * 1024;

/// Fewest distinct pixel values the stock image must hold: with fewer (a fill, a two-colour
/// pattern), its published hash could be inverted by trying the few images it could be.
pub const MIN_IMAGE_DISTINCT_PIXELS: usize = 16;

/// Fewest pixels of the stock image that [`unpredicted_pixels`] counts: a fill or a smooth gradient
/// has none, so its published hash could be inverted by trying its few parameters, while
/// anti-aliased text or dither has hundreds. A heuristic against accidental leaks, like the
/// replacement windows' rules; review is the backstop.
pub const MIN_IMAGE_UNPREDICTED_PIXELS: usize = 64;

/// An edit to one RGB565 image (16-bit little-endian pixels, rows `width` pixels apart).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageEdit {
    /// Decoded-application offset of the image's first pixel.
    pub offset: u64,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// SHA-256 of the whole stock image (`width * height * 2` bytes from `offset`).
    pub sha256: String,
    /// The box to erase before the glyph is drawn, if any.
    #[serde(default)]
    pub erase: Option<PixelBox>,
    /// The glyph to draw.
    pub glyph: Glyph,
    /// Why this image changes.
    pub purpose: String,
}

/// A rectangle of pixels inside an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PixelBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// One pixel position inside an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pixel {
    pub x: u32,
    pub y: u32,
}

/// A glyph: the author's own coverage mask, drawn in the colour of one stock pixel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Glyph {
    /// Where the mask goes in the image.
    pub at: PixelBox,
    /// One hexadecimal digit per pixel, row by row: `0` leaves the pixel as it is, `f` paints it
    /// fully in the glyph colour, and the digits between blend linearly.
    pub alpha_hex: String,
    /// The stock pixel whose colour the glyph is drawn in, read before anything changes (for a
    /// relabelled button, a pixel inside a stroke of the old label).
    pub colour_from: Pixel,
}

/// Why an image edit is malformed; `index` is its position in `image_edits`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImageEditError {
    #[error("image_edits[{index}] has an empty image")]
    EmptyImage { index: usize },
    #[error("image_edits[{index}]: the image is larger than {MAX_IMAGE_BYTES} bytes")]
    ImageTooLarge { index: usize },
    #[error("image_edits[{index}] extends past the 64-bit offset range")]
    OffsetOverflow { index: usize },
    #[error("image_edits[{index}].{field} does not lie inside the {width}x{height} image{needs}")]
    OutsideImage {
        index: usize,
        field: &'static str,
        width: u32,
        height: u32,
        needs: &'static str,
    },
    #[error(
        "image_edits[{index}].glyph.alpha_hex must hold one hex digit per glyph pixel \
         ({expected}), found {found} characters"
    )]
    AlphaLength {
        index: usize,
        expected: u64,
        found: usize,
    },
    #[error("image_edits[{index}].glyph.alpha_hex must be hexadecimal digits")]
    AlphaNotHex { index: usize },
    #[error("image_edits[{index}].glyph.alpha_hex paints no pixel")]
    EmptyGlyph { index: usize },
    #[error("image_edits[{index}].purpose must not be empty")]
    EmptyPurpose { index: usize },
    #[error("image_edits[{index}] starts before the end of image_edits[{previous}]")]
    UnorderedOrOverlapping { index: usize, previous: usize },
    #[error(
        "image_edits[{index}] overlaps the precondition window of replacements[{replacement}]; \
         overlapping windows would reveal stock bytes one hash at a time"
    )]
    OverlapsReplacement { index: usize, replacement: usize },
}

impl PixelBox {
    fn fits(&self, width: u32, height: u32) -> bool {
        self.width > 0
            && self.height > 0
            && u64::from(self.x) + u64::from(self.width) <= u64::from(width)
            && u64::from(self.y) + u64::from(self.height) <= u64::from(height)
    }

    /// The columns of the box.
    pub fn columns(&self) -> Range<u32> {
        self.x..self.x + self.width
    }

    /// The rows of the box.
    pub fn rows(&self) -> Range<u32> {
        self.y..self.y + self.height
    }
}

impl ImageEdit {
    /// The image's size in bytes, or `None` if it overflows.
    pub fn len(&self) -> Option<u64> {
        u64::from(self.width)
            .checked_mul(u64::from(self.height))?
            .checked_mul(2)
    }

    /// Whether the image is empty (never true for a validated edit).
    pub fn is_empty(&self) -> bool {
        self.len() == Some(0)
    }

    /// The image's bytes in the decoded application, its precondition window, or `None` if it does
    /// not fit the 64-bit range.
    pub fn window(&self) -> Option<Range<u64>> {
        Some(self.offset..self.offset.checked_add(self.len()?)?)
    }

    /// The glyph's coverage per pixel, row by row (`None` unless the mask is valid hex).
    pub fn alpha(&self) -> Option<Vec<u8>> {
        self.glyph
            .alpha_hex
            .chars()
            .map(|digit| digit.to_digit(16).map(|value| value as u8))
            .collect()
    }

    /// For each image row the edit may change, the columns it may change: the span covering the
    /// erase box and the glyph box on that row (so any columns between two separated boxes are
    /// included). A validated edit only; rows in ascending order.
    pub fn changed_rows(&self) -> Vec<(u32, Range<u32>)> {
        let boxes: Vec<PixelBox> = self.erase.iter().copied().chain([self.glyph.at]).collect();
        let (first, last) = boxes.iter().fold((u32::MAX, 0), |(first, last), b| {
            (first.min(b.y), last.max(b.y + b.height))
        });
        (first..last)
            .filter_map(|row| {
                let covering = boxes.iter().filter(|b| b.rows().contains(&row));
                let (start, end) = covering.fold((u32::MAX, 0), |(start, end), b| {
                    (start.min(b.x), end.max(b.x + b.width))
                });
                (start < end).then_some((row, start..end))
            })
            .collect()
    }

    /// The erased row: `width` pixels interpolated linearly between `left` and `right`, the stock
    /// pixels just outside the box, excluding both.
    pub fn erase_row(left: u16, right: u16, width: u32) -> Vec<u16> {
        // The stock pixels sit at positions 0 and width + 1, the box's at 1..=width.
        let span = width + 1;
        (0..width)
            .map(|i| {
                let mix = |l: u32, r: u32| (l * (width - i) + r * (i + 1) + span / 2) / span;
                pack(channels(left).zip(channels(right), mix))
            })
            .collect()
    }

    /// Static checks that need no firmware. `index` names the edit in errors.
    ///
    /// # Errors
    ///
    /// The first failed check, as an [`ImageEditError`].
    pub fn validate(&self, index: usize) -> Result<(), ImageEditError> {
        let len = self.len().ok_or(ImageEditError::ImageTooLarge { index })?;
        if len == 0 {
            return Err(ImageEditError::EmptyImage { index });
        }
        if len > MAX_IMAGE_BYTES {
            return Err(ImageEditError::ImageTooLarge { index });
        }
        self.window()
            .ok_or(ImageEditError::OffsetOverflow { index })?;
        let outside = |field, needs| ImageEditError::OutsideImage {
            index,
            field,
            width: self.width,
            height: self.height,
            needs,
        };
        if let Some(erase) = &self.erase {
            // Interpolation needs a stock pixel on each side of every row.
            let inner =
                erase.x >= 1 && u64::from(erase.x) + u64::from(erase.width) < u64::from(self.width);
            if !erase.fits(self.width, self.height) || !inner {
                return Err(outside(
                    "erase",
                    ", with a column on each side (the erase interpolates between them)",
                ));
            }
        }
        if !self.glyph.at.fits(self.width, self.height) {
            return Err(outside("glyph.at", ""));
        }
        let from = self.glyph.colour_from;
        if from.x >= self.width || from.y >= self.height {
            return Err(outside("glyph.colour_from", ""));
        }
        let expected = u64::from(self.glyph.at.width) * u64::from(self.glyph.at.height);
        if self.glyph.alpha_hex.len() as u64 != expected {
            return Err(ImageEditError::AlphaLength {
                index,
                expected,
                found: self.glyph.alpha_hex.len(),
            });
        }
        let alpha = self.alpha().ok_or(ImageEditError::AlphaNotHex { index })?;
        if alpha.iter().all(|&a| a == 0) {
            return Err(ImageEditError::EmptyGlyph { index });
        }
        if self.purpose.trim().is_empty() {
            return Err(ImageEditError::EmptyPurpose { index });
        }
        Ok(())
    }
}

/// The glyph colour `colour` laid over `background` with coverage `alpha` (0..=15).
pub fn blend(background: u16, colour: u16, alpha: u8) -> u16 {
    let alpha = u32::from(alpha.min(15));
    let mix = |b: u32, c: u32| (b * (15 - alpha) + c * alpha + 7) / 15;
    pack(channels(background).zip(channels(colour), mix))
}

/// Red (5 bits), green (6) and blue (5) of an RGB565 pixel.
#[derive(Clone, Copy)]
struct Channels([u32; 3]);

fn channels(pixel: u16) -> Channels {
    let pixel = u32::from(pixel);
    Channels([pixel >> 11, (pixel >> 5) & 0x3f, pixel & 0x1f])
}

impl Channels {
    fn zip(self, other: Channels, mix: impl Fn(u32, u32) -> u32) -> Channels {
        Channels([0, 1, 2].map(|i| mix(self.0[i], other.0[i])))
    }
}

fn pack(Channels([red, green, blue]): Channels) -> u16 {
    ((red << 11) | (green << 5) | blue) as u16
}

/// How predictable a stock image is: the two measures behind the rule an image must pass before
/// its hash may be published.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Predictability {
    /// Distinct pixel values.
    pub distinct: usize,
    /// Pixels its neighbours do not predict ([`unpredicted_pixels`]).
    pub unpredicted: usize,
}

impl Predictability {
    /// Whether the image passes: at least [`MIN_IMAGE_DISTINCT_PIXELS`] distinct values and
    /// [`MIN_IMAGE_UNPREDICTED_PIXELS`] unpredicted pixels.
    pub fn passes(&self) -> bool {
        self.distinct >= MIN_IMAGE_DISTINCT_PIXELS
            && self.unpredicted >= MIN_IMAGE_UNPREDICTED_PIXELS
    }
}

/// Both measures of an RGB565 image (`pixels`, rows `width` apart).
pub fn predictability(pixels: &[u16], width: usize) -> Predictability {
    let mut values = pixels.to_vec();
    values.sort_unstable();
    values.dedup();
    Predictability {
        distinct: values.len(),
        unpredicted: unpredicted_pixels(pixels, width),
    }
}

/// How many pixels of an RGB565 image (`pixels`, rows `width` apart) its neighbours do not
/// predict: per channel, the median edge predictor (from the left, upper and upper-left pixels)
/// misses by more than 1. The first row and column are not counted. Fills and linear gradients
/// score 0; edges, anti-aliased text and dither score high.
pub fn unpredicted_pixels(pixels: &[u16], width: usize) -> usize {
    let median = |left: u32, up: u32, corner: u32| {
        if corner >= left.max(up) {
            left.min(up)
        } else if corner <= left.min(up) {
            left.max(up)
        } else {
            left + up - corner
        }
    };
    if width == 0 {
        return 0;
    }
    let rows = pixels.len() / width;
    let mut count = 0;
    for y in 1..rows {
        for x in 1..width {
            let at = |dx: usize, dy: usize| channels(pixels[(y - dy) * width + x - dx]).0;
            let (left, up, corner, value) = (at(1, 0), at(0, 1), at(1, 1), at(0, 0));
            let missed = (0..3).any(|c| median(left[c], up[c], corner[c]).abs_diff(value[c]) > 1);
            count += usize::from(missed);
        }
    }
    count
}
