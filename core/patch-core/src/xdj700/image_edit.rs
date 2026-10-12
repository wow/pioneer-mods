//! Image edits of schema-v2 recipes on the stock application ([`patch_schema::ImageEdit`]): where
//! an image lies and the edited pixels. The edit reads every stock pixel it uses from the owner's
//! file; the recipe carries only coordinates, the author's own art, mask and colours, and no hash
//! of the image (the recipe's output pin checks the edit instead). In order: every stock colour
//! the edit names is read and the author's colours are checked against the stock pixels beneath
//! them, then the box is erased, the art painted and the glyph drawn.

use super::recipe_checks::{bounded, out_of_bounds};
use super::recipe_error::RecipeError;
use patch_schema::{ColourSource, Glyph, ImageEdit, Paint, PaletteEntry, WindowOwner, blend};
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

/// The paint step: the art, its palette's colours (`None` keeps the pixel) and each pixel's entry.
struct PaintStep<'a> {
    paint: &'a Paint,
    palette: Vec<Option<u16>>,
    indices: Vec<u8>,
}

/// The glyph step: the glyph, its colour and its coverage per pixel.
struct GlyphStep<'a> {
    glyph: &'a Glyph,
    colour: u16,
    alpha: Vec<u8>,
}

/// An RGB565 image in place: 16-bit little-endian pixels, rows `width` pixels apart.
struct Pixels<'a> {
    bytes: &'a mut [u8],
    width: usize,
}

impl Pixels<'_> {
    fn at(&self, x: u32, y: u32) -> usize {
        (y as usize * self.width + x as usize) * 2
    }

    fn get(&self, x: u32, y: u32) -> u16 {
        let i = self.at(x, y);
        u16::from_le_bytes([self.bytes[i], self.bytes[i + 1]])
    }

    fn put(&mut self, x: u32, y: u32, pixel: u16) {
        let i = self.at(x, y);
        self.bytes[i..i + 2].copy_from_slice(&pixel.to_le_bytes());
    }
}

/// Applies edit `index` to the image at `range` of `decoded`, which still holds the stock image
/// there, and returns the decoded ranges it may have changed (one per changed row, for the
/// bounded-diff check). `edit` is validated and `range` checked.
///
/// # Errors
///
/// [`RecipeError::PaintMatchesStock`]: the author's colours mostly equal the stock pixels they
/// replace, which would publish them.
pub(super) fn apply_image_edit(
    index: usize,
    edit: &ImageEdit,
    range: Range<usize>,
    decoded: &mut [u8],
) -> Result<Vec<Range<usize>>, RecipeError> {
    let width = edit.width as usize;
    let mut image = Pixels {
        bytes: &mut decoded[range.clone()],
        width,
    };
    // Every stock colour the edit uses, read before anything changes, with each step's data.
    let paint = edit.paint.as_ref().map(|paint| {
        let palette: Vec<Option<u16>> = paint
            .palette
            .iter()
            .map(|entry| match entry {
                PaletteEntry::Keep => None,
                PaletteEntry::Colour(colour) => Some(colour.0),
                PaletteEntry::From(from) => Some(image.get(from.x, from.y)),
            })
            .collect();
        let indices = paint.indices().expect("validated digits");
        PaintStep {
            paint,
            palette,
            indices,
        }
    });
    let glyph = edit.glyph.as_ref().map(|glyph| {
        let colour = match glyph.colour {
            ColourSource::Colour(colour) => colour.0,
            ColourSource::From(from) => image.get(from.x, from.y),
        };
        let alpha = edit.alpha().expect("validated mask");
        GlyphStep {
            glyph,
            colour,
            alpha,
        }
    });
    check_own_colours(index, &image, paint.as_ref(), glyph.as_ref())?;

    if let Some(erase) = &edit.erase {
        for y in erase.rows() {
            let left = image.get(erase.x - 1, y);
            let right = image.get(erase.x + erase.width, y);
            let row = ImageEdit::erase_row(left, right, erase.width);
            for (x, pixel) in erase.columns().zip(row) {
                image.put(x, y, pixel);
            }
        }
    }
    if let Some(PaintStep {
        paint,
        palette,
        indices,
    }) = &paint
    {
        for (i, &entry) in indices.iter().enumerate() {
            if let Some(colour) = palette[usize::from(entry)] {
                let (x, y) = in_box(paint.at.x, paint.at.y, paint.at.width, i);
                image.put(x, y, colour);
            }
        }
    }
    if let Some(GlyphStep {
        glyph,
        colour,
        alpha,
    }) = &glyph
    {
        for (i, &a) in alpha.iter().enumerate().filter(|(_, a)| **a > 0) {
            let (x, y) = in_box(glyph.at.x, glyph.at.y, glyph.at.width, i);
            let pixel = blend(image.get(x, y), *colour, a);
            image.put(x, y, pixel);
        }
    }
    Ok(edit
        .changed_rows()
        .into_iter()
        .map(|(y, columns)| {
            range.start + image.at(columns.start, y)..range.start + image.at(columns.end, y)
        })
        .collect())
}

/// Pixel `i` of a box `box_width` wide at (`x`, `y`), row by row.
fn in_box(x: u32, y: u32, box_width: u32, i: usize) -> (u32, u32) {
    let box_width = box_width as usize;
    (x + (i % box_width) as u32, y + (i / box_width) as u32)
}

/// Refuses author's colours (each palette `colour` entry, and a glyph's `colour` where it fully
/// covers) that more than half match the stock pixels beneath them: such a recipe
/// would publish those pixels. `from` and `keep` name stock pixels by position and are exempt.
fn check_own_colours(
    index: usize,
    image: &Pixels<'_>,
    paint: Option<&PaintStep<'_>>,
    glyph: Option<&GlyphStep<'_>>,
) -> Result<(), RecipeError> {
    let refuse = |field: String, matching: usize, painted: usize| {
        if matching * 2 > painted {
            Err(RecipeError::PaintMatchesStock {
                index,
                field,
                matching,
                painted,
            })
        } else {
            Ok(())
        }
    };
    if let Some(PaintStep { paint, indices, .. }) = paint {
        let mut counts = vec![(0usize, 0usize); paint.palette.len()];
        for (i, &entry) in indices.iter().enumerate() {
            if let PaletteEntry::Colour(colour) = paint.palette[usize::from(entry)] {
                let (x, y) = in_box(paint.at.x, paint.at.y, paint.at.width, i);
                let count = &mut counts[usize::from(entry)];
                count.0 += usize::from(image.get(x, y) == colour.0);
                count.1 += 1;
            }
        }
        // Each entry at most half: then all of them together are too.
        for (entry, &(matching, painted)) in counts.iter().enumerate() {
            refuse(format!("paint.palette[{entry}]"), matching, painted)?;
        }
    }
    if let Some(GlyphStep {
        glyph,
        colour,
        alpha,
    }) = glyph
        && let ColourSource::Colour(_) = glyph.colour
    {
        let full = alpha.iter().enumerate().filter(|(_, a)| **a == 15);
        let (matching, painted) = full.fold((0, 0), |(m, p), (i, _)| {
            let (x, y) = in_box(glyph.at.x, glyph.at.y, glyph.at.width, i);
            (m + usize::from(image.get(x, y) == *colour), p + 1)
        });
        refuse("glyph.colour".to_owned(), matching, painted)?;
    }
    Ok(())
}
