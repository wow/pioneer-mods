//! The bounded-diff check of the owner-input tests: where a committed recipe's output may differ
//! from the stock application, derived from the recipe alone (included with `#[path]`). Compares
//! offsets only; prints no bytes.

use crate::official_pins::VERSION_TEXT;
use patch_core::{parse_upd, xdj700::decode_application};
use patch_schema::RecipeV2;

/// The offsets at which the decoded application of the update `rebuilt` differs from `stock`.
pub fn changed_offsets(stock: &[u8], rebuilt: &[u8]) -> Vec<usize> {
    let output = decode_application(&parse_upd(rebuilt).expect("parse")).expect("decode");
    assert_eq!(output.decoded().len(), stock.len());
    (stock.iter().zip(output.decoded()))
        .enumerate()
        .filter(|(_, (stock, output))| stock != output)
        .map(|(offset, _)| offset)
        .collect()
}

/// Checks that every offset in `changed` lies in the version string, in a byte a replacement of
/// `recipe` writes (not a kept `--` byte), or in the erase or glyph box of one of its image edits,
/// and that every replacement and every image edit changes something. Derived from the recipe
/// alone, independently of the engine's own bounded diff. Returns the offsets outside the images.
pub fn changes_outside_images(name: &str, recipe: &RecipeV2, changed: &[usize]) -> Vec<usize> {
    let written: Vec<(usize, usize)> = (recipe.replacements.iter().enumerate())
        .flat_map(|(index, replacement)| {
            let start = usize::try_from(replacement.offset).expect("inside the application");
            let pattern = replacement.pattern().expect("validated");
            let at = (0..pattern.len()).filter(move |&at| pattern[at].is_some());
            at.map(move |at| (start + at, index))
        })
        .collect();
    let mut replacements_changed = vec![false; recipe.replacements.len()];
    let mut images_changed = vec![false; recipe.image_edits.len()];
    let mut outside_images = Vec::new();
    for &offset in changed {
        if let Some(image) = label_box_holding(recipe, offset) {
            images_changed[image] = true;
            continue;
        }
        outside_images.push(offset);
        match written.iter().find(|&&(at, _)| at == offset) {
            Some(&(_, index)) => replacements_changed[index] = true,
            None => assert!(
                VERSION_TEXT.contains(&offset),
                "{name}: {offset:#x} changes, but the recipe does not declare it"
            ),
        }
    }
    assert!(
        !replacements_changed.contains(&false),
        "{name}: a replacement changes nothing"
    );
    assert!(
        !images_changed.contains(&false),
        "{name}: an image edit changes nothing"
    );
    outside_images
}

/// The index of the image edit of `recipe` whose erase box or glyph box holds the pixel at
/// decoded `offset`, if any.
fn label_box_holding(recipe: &RecipeV2, offset: usize) -> Option<usize> {
    recipe.image_edits.iter().position(|edit| {
        let Some(at) = (offset as u64).checked_sub(edit.offset) else {
            return false;
        };
        let pixel = at / 2;
        let (x, y) = (pixel % u64::from(edit.width), pixel / u64::from(edit.width));
        let boxes = [edit.erase.as_ref(), Some(&edit.glyph.at)];
        y < u64::from(edit.height)
            && boxes.into_iter().flatten().any(|pixels| {
                u32::try_from(x).is_ok_and(|x| pixels.columns().contains(&x))
                    && u32::try_from(y).is_ok_and(|y| pixels.rows().contains(&y))
            })
    })
}
