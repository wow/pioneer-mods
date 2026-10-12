//! Image edits of schema-v2 recipes on the stock application ([`patch_schema::ImageEdit`]): where
//! an image lies and the edited pixels. The edit reads every stock pixel it uses from the owner's
//! file; the recipe carries only coordinates, the author's own art, mask and colours, and no hash
//! of the image (the recipe's output pin checks the edit instead). In order: every stock colour
//! the edit names is read, then the box is erased, the art painted and the glyph drawn.

use super::recipe_checks::{bounded, out_of_bounds};
use super::recipe_error::RecipeError;
use patch_schema::{ImageEdit, PaletteEntry, WindowOwner, blend};
use std::ops::Range;

/// The edit's image, if it lies inside an application of `application_len`.
pub(super) fn image_range(
    index: usize,
    edit: &ImageEdit,
    application_len: usize,
) -> Result<Range<usize>, RecipeError> {
    bounded(
        edit.window(),
        application_len,
        out_of_bounds(WindowOwner::ImageEdit(index), application_len),
    )
}

/// Applies `edit` to the image at `range` of `decoded`, which still holds the stock image there,
/// and returns the decoded ranges it may have changed (one per changed row, for the bounded-diff
/// check). `edit` is validated and `range` checked.
pub(super) fn apply_image_edit(
    edit: &ImageEdit,
    range: Range<usize>,
    decoded: &mut [u8],
) -> Vec<Range<usize>> {
    let image = &mut decoded[range.clone()];
    let width = edit.width as usize;
    let at = |x: u32, y: u32| (y as usize * width + x as usize) * 2;
    let get = |image: &[u8], x: u32, y: u32| {
        let i = at(x, y);
        u16::from_le_bytes([image[i], image[i + 1]])
    };
    let put = |image: &mut [u8], x: u32, y: u32, pixel: u16| {
        let i = at(x, y);
        image[i..i + 2].copy_from_slice(&pixel.to_le_bytes());
    };
    // Every stock colour the edit uses is read before anything changes.
    let glyph_colour = edit
        .glyph
        .as_ref()
        .map(|glyph| match (glyph.colour, glyph.colour_from) {
            (Some(colour), _) => colour.0,
            (None, Some(from)) => get(image, from.x, from.y),
            (None, None) => unreachable!("validated glyph colour"),
        });
    let palette: Option<Vec<Option<u16>>> = edit.paint.as_ref().map(|paint| {
        paint
            .palette
            .iter()
            .map(|entry| match entry {
                PaletteEntry::Keep => None,
                PaletteEntry::Colour(colour) => Some(colour.0),
                PaletteEntry::From(from) => Some(get(image, from.x, from.y)),
            })
            .collect()
    });
    if let Some(erase) = &edit.erase {
        for y in erase.rows() {
            let left = get(image, erase.x - 1, y);
            let right = get(image, erase.x + erase.width, y);
            let row = ImageEdit::erase_row(left, right, erase.width);
            for (x, pixel) in erase.columns().zip(row) {
                put(image, x, y, pixel);
            }
        }
    }
    if let (Some(paint), Some(palette)) = (&edit.paint, &palette) {
        let box_width = paint.at.width as usize;
        let digits = paint
            .pixels_hex
            .chars()
            .map(|d| d.to_digit(16).expect("validated digit"));
        for (i, digit) in digits.enumerate() {
            if let Some(colour) = palette[digit as usize] {
                let x = paint.at.x + (i % box_width) as u32;
                let y = paint.at.y + (i / box_width) as u32;
                put(image, x, y, colour);
            }
        }
    }
    if let (Some(glyph), Some(colour)) = (&edit.glyph, glyph_colour) {
        let alpha = edit.alpha().expect("validated mask");
        for (i, &a) in alpha.iter().enumerate() {
            if a == 0 {
                continue;
            }
            let x = glyph.at.x + (i % glyph.at.width as usize) as u32;
            let y = glyph.at.y + (i / glyph.at.width as usize) as u32;
            let pixel = blend(get(image, x, y), colour, a);
            put(image, x, y, pixel);
        }
    }
    edit.changed_rows()
        .into_iter()
        .map(|(y, columns)| range.start + at(columns.start, y)..range.start + at(columns.end, y))
        .collect()
}
