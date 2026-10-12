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
    /// The palette index of each pixel, row by row (`None` unless every digit is hex). The one
    /// place the digits are read: validation and the engine both use it.
    pub fn indices(&self) -> Option<Vec<u8>> {
        self.pixels_hex
            .chars()
            .map(|digit| digit.to_digit(16).map(|index| index as u8))
            .collect()
    }

    /// The palette entry of each pixel, row by row (`None` unless every digit is hex and names
    /// an entry).
    pub fn entries(&self) -> Option<Vec<PaletteEntry>> {
        self.indices()?
            .into_iter()
            .map(|index| self.palette.get(usize::from(index)).copied())
            .collect()
    }

    /// Whether `other` paints the same: the same box, and on every pixel the same entry, however
    /// either palette is ordered and whatever unused entries it holds.
    pub fn same(&self, other: &Paint) -> bool {
        self.at == other.at
            && matches!((self.entries(), other.entries()), (Some(a), Some(b)) if a == b)
    }

    /// Static checks for an image of `width` x `height`; `index` names the edit in errors.
    pub(crate) fn validate(
        &self,
        index: usize,
        width: u32,
        height: u32,
    ) -> Result<(), ImageEditError> {
        if !self.at.fits(width, height) {
            return Err(ImageEditError::OutsideImage {
                index,
                field: "paint.at",
                width,
                height,
                needs: "",
            });
        }
        let len = self.palette.len();
        if len == 0 || len > MAX_PALETTE {
            return Err(ImageEditError::PaletteSize { index, len });
        }
        for (entry, palette_entry) in self.palette.iter().enumerate() {
            if let PaletteEntry::From(Pixel { x, y }) = *palette_entry
                && (x >= width || y >= height)
            {
                return Err(ImageEditError::FromOutsideImage {
                    index,
                    entry,
                    x,
                    y,
                    width,
                    height,
                });
            }
        }
        // One pass over the digits: their count, the first that names no entry, and whether any
        // pixel changes.
        let (mut found, mut paints) = (0usize, false);
        for digit in self.pixels_hex.chars() {
            found += 1;
            let entry = digit
                .to_digit(16)
                .and_then(|entry| self.palette.get(entry as usize));
            match entry {
                None => return Err(ImageEditError::PaintDigit { index, digit, len }),
                Some(entry) => paints |= *entry != PaletteEntry::Keep,
            }
        }
        let expected = u64::from(self.at.width) * u64::from(self.at.height);
        if found as u64 != expected {
            return Err(ImageEditError::PaintLength {
                index,
                expected,
                found,
            });
        }
        if !paints {
            return Err(ImageEditError::PaintsNothing { index });
        }
        Ok(())
    }
}
