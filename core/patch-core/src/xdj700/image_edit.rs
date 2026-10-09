//! Image edits of schema-v2 recipes on the stock application ([`patch_schema::ImageEdit`]): where
//! an image lies, whether it is the stock image the recipe was written for, and the edited pixels.
//!
//! An image's precondition is the SHA-256 of the whole stock image, which the recipe publishes. So
//! the image must hold at least [`patch_schema::MIN_IMAGE_DISTINCT_PIXELS`] distinct pixel values
//! and [`patch_schema::MIN_IMAGE_UNPREDICTED_PIXELS`] pixels its neighbours do not predict: a fill,
//! a two-colour pattern or a smooth gradient could be recovered from its hash by trying its few
//! parameters. The edit reads every pixel it uses from the owner's file; the recipe carries only
//! coordinates and the author's own glyph mask.

use super::precondition::DeclaredHash;
use super::recipe_error::RecipeError;
use crate::identity::sha256_hex;
use patch_schema::{ImageEdit, blend, predictability};
use std::ops::Range;

/// The edit's image, if it lies inside an application of `application_len`.
pub(super) fn image_range(
    index: usize,
    edit: &ImageEdit,
    application_len: usize,
) -> Result<Range<usize>, RecipeError> {
    // `validate` has checked that the window fits the 64-bit range.
    let Range { start, end } = edit.window().unwrap_or(u64::MAX..u64::MAX);
    let out_of_bounds = || RecipeError::ImageOutOfBounds {
        index,
        end,
        len: application_len,
    };
    let start = usize::try_from(start).map_err(|_| out_of_bounds())?;
    let end_usize = usize::try_from(end).map_err(|_| out_of_bounds())?;
    if end_usize > application_len {
        return Err(out_of_bounds());
    }
    Ok(start..end_usize)
}

/// The edit's image on the stock application, after every per-image rule: inside the application,
/// matching the declared hash when `declared` asks for it, and not predictable. Applying a recipe
/// and computing a draft's hashes both go through it. The hash is compared first, so a mistyped
/// offset reads as a mismatch.
pub(super) fn checked_image(
    index: usize,
    edit: &ImageEdit,
    stock: &[u8],
    declared: DeclaredHash,
) -> Result<Range<usize>, RecipeError> {
    let range = image_range(index, edit, stock.len())?;
    if declared == DeclaredHash::Compare
        && !sha256_hex(&stock[range.clone()]).eq_ignore_ascii_case(&edit.sha256)
    {
        return Err(RecipeError::ImagePrecondition {
            index,
            expected: edit.sha256.clone(),
        });
    }
    let image: Vec<u16> = pixels(&stock[range.clone()]).collect();
    let measured = predictability(&image, edit.width as usize);
    if !measured.passes() {
        return Err(RecipeError::PredictableImage {
            index,
            distinct: measured.distinct,
            unpredicted: measured.unpredicted,
        });
    }
    Ok(range)
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
    // Read before anything changes.
    let colour = get(image, edit.glyph.colour_from.x, edit.glyph.colour_from.y);
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
    let alpha = edit.alpha().expect("validated mask");
    let glyph = edit.glyph.at;
    for (i, &a) in alpha.iter().enumerate() {
        if a == 0 {
            continue;
        }
        let x = glyph.x + (i % glyph.width as usize) as u32;
        let y = glyph.y + (i / glyph.width as usize) as u32;
        let pixel = blend(get(image, x, y), colour, a);
        put(image, x, y, pixel);
    }
    edit.changed_rows()
        .into_iter()
        .map(|(y, columns)| range.start + at(columns.start, y)..range.start + at(columns.end, y))
        .collect()
}

fn pixels(bytes: &[u8]) -> impl Iterator<Item = u16> + '_ {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
}
