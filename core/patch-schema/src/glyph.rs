//! A glyph ([`Glyph`]): the author's own coverage mask in an image edit, drawn in the author's
//! own colour or in that of a stock pixel named by its position, never by its value.

use crate::image::{ImageEdit, ImageEditError, Pixel, PixelBox};
use crate::paint::Rgb565;
use serde::{Deserialize, Serialize};

/// Where a glyph's colour comes from: the author's own, or a stock pixel's by its position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColourSource {
    Colour(Rgb565),
    From(Pixel),
}

/// A glyph: the author's own coverage mask, drawn in the author's own colour or in the colour of
/// one stock pixel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "GlyphFile", into = "GlyphFile")]
pub struct Glyph {
    /// Where the mask goes in the image.
    pub at: PixelBox,
    /// One hexadecimal digit per pixel, row by row: `0` leaves the pixel as it is, `f` paints it
    /// fully in the glyph colour, and the digits between blend linearly.
    pub alpha_hex: String,
    /// The glyph colour: the author's own (`colour`), or that of a stock pixel read before
    /// anything changes (`colour_from`; for a relabelled button, a pixel inside a stroke of the
    /// old label).
    pub colour: ColourSource,
}

/// A glyph as written: exactly one of `colour_from` and `colour`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GlyphFile {
    at: PixelBox,
    alpha_hex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    colour_from: Option<Pixel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    colour: Option<Rgb565>,
}

impl TryFrom<GlyphFile> for Glyph {
    type Error = &'static str;

    fn try_from(file: GlyphFile) -> Result<Self, Self::Error> {
        let colour = match (file.colour, file.colour_from) {
            (Some(colour), None) => ColourSource::Colour(colour),
            (None, Some(from)) => ColourSource::From(from),
            _ => return Err("a glyph needs exactly one of colour and colour_from"),
        };
        Ok(Glyph {
            at: file.at,
            alpha_hex: file.alpha_hex,
            colour,
        })
    }
}

impl From<Glyph> for GlyphFile {
    fn from(glyph: Glyph) -> GlyphFile {
        let (colour, colour_from) = match glyph.colour {
            ColourSource::Colour(colour) => (Some(colour), None),
            ColourSource::From(from) => (None, Some(from)),
        };
        GlyphFile {
            at: glyph.at,
            alpha_hex: glyph.alpha_hex,
            colour_from,
            colour,
        }
    }
}

impl ImageEdit {
    pub(crate) fn validate_glyph(&self, index: usize, glyph: &Glyph) -> Result<(), ImageEditError> {
        let outside = |field| ImageEditError::OutsideImage {
            index,
            field,
            width: self.width,
            height: self.height,
            needs: "",
        };
        if !glyph.at.fits(self.width, self.height) {
            return Err(outside("glyph.at"));
        }
        if let ColourSource::From(from) = glyph.colour
            && (from.x >= self.width || from.y >= self.height)
        {
            return Err(outside("glyph.colour_from"));
        }
        let expected = u64::from(glyph.at.width) * u64::from(glyph.at.height);
        let found = glyph.alpha_hex.chars().count();
        if found as u64 != expected {
            return Err(ImageEditError::AlphaLength {
                index,
                expected,
                found,
            });
        }
        let alpha = self.alpha().ok_or(ImageEditError::AlphaNotHex { index })?;
        if alpha.iter().all(|&a| a == 0) {
            return Err(ImageEditError::EmptyGlyph { index });
        }
        Ok(())
    }
}
