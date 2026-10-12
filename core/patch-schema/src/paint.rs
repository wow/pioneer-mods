//! Painting the author's own art into an image ([`Paint`]): one hexadecimal digit per pixel
//! names an entry of a palette of at most 16, each the author's own colour, the colour of one
//! stock pixel the recipe names by its position (read from the owner's file at patch time), or
//! the stock pixel left as it is. A skin paints its own pads this way; the recipe carries no stock
//! pixel, and a stock colour it needs (the key colour that lets the screen show through) is named
//! by position, never by value.

use crate::image::{ImageEditError, Pixel, PixelBox};
use serde::{Deserialize, Serialize};

/// Most entries a palette may hold: one hex digit names one.
pub const MAX_PALETTE: usize = 16;

/// An RGB565 colour the author chose, written `0x` and four hexadecimal digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Rgb565(pub u16);

impl TryFrom<String> for Rgb565 {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let digits = text
            .strip_prefix("0x")
            .filter(|d| d.len() == 4 && d.chars().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| format!("{text:?} is not an RGB565 colour (0x and four hex digits)"))?;
        u16::from_str_radix(digits, 16)
            .map(Rgb565)
            .map_err(|error| error.to_string())
    }
}

impl From<Rgb565> for String {
    fn from(colour: Rgb565) -> String {
        format!("0x{:04x}", colour.0)
    }
}

/// One palette entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PaletteEntry {
    /// The stock pixel is left as it is.
    Keep,
    /// The author's own colour.
    Colour(Rgb565),
    /// The colour of the stock pixel at this position, read before anything changes.
    From(Pixel),
}

/// The author's own art, painted into a box of the image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paint {
    /// Where the art goes in the image.
    pub at: PixelBox,
    /// The palette the art's digits name, 1 to 16 entries.
    pub palette: Vec<PaletteEntry>,
    /// One hexadecimal digit per pixel of `at`, row by row: the palette entry painted there.
    pub pixels_hex: String,
}

impl Paint {
    /// The palette entry of each pixel, row by row (`None` unless every digit is hex and names
    /// an entry).
    pub fn entries(&self) -> Option<Vec<PaletteEntry>> {
        self.pixels_hex
            .chars()
            .map(|digit| {
                let index = digit.to_digit(16)? as usize;
                self.palette.get(index).copied()
            })
            .collect()
    }

    /// Whether `other` paints the same: the same box, palette and digits (compared without regard
    /// to case, as [`Paint::entries`] reads them).
    pub fn same(&self, other: &Paint) -> bool {
        (self.at, &self.palette) == (other.at, &other.palette)
            && self.pixels_hex.eq_ignore_ascii_case(&other.pixels_hex)
    }

    /// Static checks for an image of `width` x `height`; `index` names the edit in errors.
    pub(crate) fn validate(
        &self,
        index: usize,
        width: u32,
        height: u32,
    ) -> Result<(), ImageEditError> {
        let outside = |field| ImageEditError::OutsideImage {
            index,
            field,
            width,
            height,
            needs: "",
        };
        if !self.at.fits(width, height) {
            return Err(outside("paint.at"));
        }
        let len = self.palette.len();
        if len == 0 || len > MAX_PALETTE {
            return Err(ImageEditError::PaletteSize { index, len });
        }
        let inside = |p: &Pixel| p.x < width && p.y < height;
        if self
            .palette
            .iter()
            .any(|entry| matches!(entry, PaletteEntry::From(p) if !inside(p)))
        {
            return Err(outside("paint.palette (a from pixel)"));
        }
        let expected = u64::from(self.at.width) * u64::from(self.at.height);
        let found = self.pixels_hex.chars().count();
        if found as u64 != expected {
            return Err(ImageEditError::PaintLength {
                index,
                expected,
                found,
            });
        }
        if let Some(digit) = self.pixels_hex.chars().find(|c| !c.is_ascii_hexdigit()) {
            return Err(ImageEditError::PaintDigit { index, digit, len });
        }
        let entries = self.entries().ok_or_else(|| {
            let digit = self
                .pixels_hex
                .chars()
                .find(|c| c.to_digit(16).is_some_and(|d| d as usize >= len))
                .unwrap_or('?');
            ImageEditError::PaintDigit { index, digit, len }
        })?;
        if entries.iter().all(|entry| *entry == PaletteEntry::Keep) {
            return Err(ImageEditError::PaintsNothing { index });
        }
        Ok(())
    }
}
