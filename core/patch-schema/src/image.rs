//! Image edits in schema-v2 recipes: changes to a 16-bit RGB565 image stored in the decoded
//! application, described so that the recipe carries **no stock pixels**.
//!
//! An edit erases a box, paints art, draws a glyph, or does several of these, in that order. The
//! erase refills each row of a box by interpolating between the stock pixels just left and right
//! of it, which removes a label from a smooth background. The paint ([`Paint`]) is the author's own
//! art: one hex digit per pixel naming a palette entry (the author's colour, a stock pixel's colour
//! by its position, or the stock pixel kept). The glyph is an alpha mask the author drew (one hex
//! digit of coverage per pixel), in the author's own colour or in that of a stock pixel named by
//! its position. Every stock colour the edit uses is read from the owner's own file at patch time,
//! before anything changes; the recipe carries none. No hash of the image is published:
//! anyone could check a guess at the stock pixels against it, and a rendered label can be easy to
//! guess. The input and the stock application are pinned by SHA-256, so the image at `offset` is
//! fixed, and a recipe with image edits must pin its output (`expected.application_sha256`), which
//! catches a changed offset or a change in the arithmetic below.
//!
//! The pixel arithmetic is fixed here so that every front end produces the same bytes:
//! [`ImageEdit::erase_row`] and [`blend`].

use crate::glyph::Glyph;
use crate::paint::Paint;
use serde::{Deserialize, Serialize};
use std::ops::Range;

/// Largest image an edit may name, in bytes (an 800x480 screen is 768,000).
pub const MAX_IMAGE_BYTES: u64 = 1024 * 1024;

/// The bytes of a `width` x `height` image of `bytes_per_pixel`-byte pixels, or `None` if it
/// overflows. Recipe image edits and screen slots both measure images with it.
pub fn image_bytes(width: u32, height: u32, bytes_per_pixel: u32) -> Option<u64> {
    u64::from(width)
        .checked_mul(u64::from(height))?
        .checked_mul(u64::from(bytes_per_pixel))
}

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
    /// The box to erase first, if any.
    #[serde(default)]
    pub erase: Option<PixelBox>,
    /// The author's own art to paint next, if any ([`Paint`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paint: Option<Paint>,
    /// The glyph to draw last, if any. An edit paints, draws a glyph, or both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<Glyph>,
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
    #[error("image_edits[{index}] neither erases, paints nor draws a glyph")]
    NothingToDraw { index: usize },
    #[error(
        "image_edits[{index}].paint.palette[{entry}].from ({x}, {y}) does not lie inside the \
         {width}x{height} image"
    )]
    FromOutsideImage {
        index: usize,
        entry: usize,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    #[error("image_edits[{index}].paint.palette must hold 1 to 16 entries, not {len}")]
    PaletteSize { index: usize, len: usize },
    #[error(
        "image_edits[{index}].paint.pixels_hex must hold one hex digit per painted pixel \
         ({expected}), found {found} characters"
    )]
    PaintLength {
        index: usize,
        expected: u64,
        found: usize,
    },
    #[error(
        "image_edits[{index}].paint.pixels_hex holds {digit:?}, which names no entry of the \
         {len}-entry palette"
    )]
    PaintDigit {
        index: usize,
        digit: char,
        len: usize,
    },
    #[error("image_edits[{index}].paint keeps every pixel")]
    PaintsNothing { index: usize },
    #[error("image_edits[{index}].purpose must not be empty")]
    EmptyPurpose { index: usize },
    #[error("image_edits[{index}] starts before the end of image_edits[{previous}]")]
    UnorderedOrOverlapping { index: usize, previous: usize },
    #[error(
        "image_edits[{index}] overlaps the precondition window of replacements[{replacement}]; \
         the window's hash would cover pixels of the edited image"
    )]
    OverlapsReplacement { index: usize, replacement: usize },
}

impl PixelBox {
    pub(crate) fn fits(&self, width: u32, height: u32) -> bool {
        self.width > 0
            && self.height > 0
            && u64::from(self.x) + u64::from(self.width) <= u64::from(width)
            && u64::from(self.y) + u64::from(self.height) <= u64::from(height)
    }

    /// The columns of the box (saturating at `u32::MAX` for a box past the 32-bit range, which
    /// validation refuses).
    pub fn columns(&self) -> Range<u32> {
        self.x..self.x.saturating_add(self.width)
    }

    /// The rows of the box (saturating like [`PixelBox::columns`]).
    pub fn rows(&self) -> Range<u32> {
        self.y..self.y.saturating_add(self.height)
    }
}

impl ImageEdit {
    /// Whether `other` is the same edit: the same image and erase box, the same paint (on every
    /// pixel the same entry, [`Paint::same`]) and the same glyph (box, colour, and mask, its hex
    /// digits compared without regard to case, as [`ImageEdit::alpha`] reads them); only the
    /// purpose may differ. A composed build applies it once.
    pub fn same_edit(&self, other: &ImageEdit) -> bool {
        let same_paint = match (&self.paint, &other.paint) {
            (Some(paint), Some(other)) => paint.same(other),
            (None, None) => true,
            _ => false,
        };
        let same_glyph = match (&self.glyph, &other.glyph) {
            (Some(glyph), Some(other)) => {
                (glyph.at, glyph.colour) == (other.at, other.colour)
                    && glyph.alpha_hex.eq_ignore_ascii_case(&other.alpha_hex)
            }
            (None, None) => true,
            _ => false,
        };
        (self.offset, self.width, self.height, self.erase)
            == (other.offset, other.width, other.height, other.erase)
            && same_paint
            && same_glyph
    }

    /// The image's size in bytes, or `None` if it overflows.
    pub fn len(&self) -> Option<u64> {
        image_bytes(self.width, self.height, 2)
    }

    /// Whether the image is empty (never true for a validated edit).
    pub fn is_empty(&self) -> bool {
        self.len() == Some(0)
    }

    /// The image's bytes in the decoded application, or `None` if they do not fit the 64-bit
    /// range.
    pub fn window(&self) -> Option<Range<u64>> {
        Some(self.offset..self.offset.checked_add(self.len()?)?)
    }

    /// The glyph's coverage per pixel, row by row (`None` without a glyph, or unless the mask is
    /// valid hex).
    pub fn alpha(&self) -> Option<Vec<u8>> {
        self.glyph
            .as_ref()?
            .alpha_hex
            .chars()
            .map(|digit| digit.to_digit(16).map(|value| value as u8))
            .collect()
    }

    /// For each image row the edit may change, the columns it may change: the span covering the
    /// erase, paint and glyph boxes on that row (so any columns between two separated boxes are
    /// included). A validated edit only; rows in ascending order.
    pub fn changed_rows(&self) -> Vec<(u32, Range<u32>)> {
        let boxes: Vec<PixelBox> = self
            .erase
            .iter()
            .copied()
            .chain(self.paint.as_ref().map(|paint| paint.at))
            .chain(self.glyph.as_ref().map(|glyph| glyph.at))
            .collect();
        let (first, last) = boxes.iter().fold((u32::MAX, 0), |(first, last), b| {
            (first.min(b.y), last.max(b.rows().end))
        });
        (first..last)
            .filter_map(|row| {
                let covering = boxes.iter().filter(|b| b.rows().contains(&row));
                let (start, end) = covering.fold((u32::MAX, 0), |(start, end), b| {
                    (start.min(b.x), end.max(b.columns().end))
                });
                (start < end).then_some((row, start..end))
            })
            .collect()
    }

    /// The erased row: `width` pixels interpolated linearly between `left` and `right`, the stock
    /// pixels just outside the box, excluding both. `width` is an erase box's width: the result
    /// holds that many pixels (64-bit arithmetic, so any width is computed without overflow).
    pub fn erase_row(left: u16, right: u16, width: u32) -> Vec<u16> {
        // The stock pixels sit at positions 0 and width + 1, the box's at 1..=width.
        let width = u64::from(width);
        let span = width + 1;
        (0..width)
            .map(|i| {
                let mix = |l: u32, r: u32| {
                    ((u64::from(l) * (width - i) + u64::from(r) * (i + 1) + span / 2) / span) as u32
                };
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
        if self.erase.is_none() && self.paint.is_none() && self.glyph.is_none() {
            return Err(ImageEditError::NothingToDraw { index });
        }
        if let Some(paint) = &self.paint {
            paint.validate(index, self.width, self.height)?;
        }
        if let Some(glyph) = &self.glyph {
            self.validate_glyph(index, glyph)?;
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
