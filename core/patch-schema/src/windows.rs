//! The windows of a schema-v2 recipe as one labelled list: each replacement's precondition window
//! (with the span it writes) and each edited image. The checks that compare windows with each
//! other, with protected ranges or across recipes read this list, so a rule for windows, or a new
//! kind of edit, is added in one place.

use crate::v2::RecipeV2;
use std::fmt;
use std::ops::Range;

/// What a window belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WindowOwner {
    /// `replacements[index]`.
    Replacement(usize),
    /// `image_edits[index]`.
    ImageEdit(usize),
}

impl fmt::Display for WindowOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WindowOwner::Replacement(index) => write!(f, "replacements[{index}]"),
            WindowOwner::ImageEdit(index) => write!(f, "image_edits[{index}]"),
        }
    }
}

/// One window, in decoded-application offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelledWindow {
    pub owner: WindowOwner,
    /// A replacement's precondition window (the bytes its hash covers), or an edited image (the
    /// bytes the edit reads; no hash covers them).
    pub window: Range<u64>,
    /// The bytes a replacement writes, inside its window; `None` for an image edit, which may
    /// change any row of its image (`ImageEdit::changed_rows` says which).
    pub span: Option<Range<u64>>,
}

impl RecipeV2 {
    /// Every window, in recipe order: the replacements' precondition windows, then the edited
    /// images.
    /// A window that does not fit the 64-bit range is left out (`validate` refuses it).
    pub fn windows(&self) -> Vec<LabelledWindow> {
        let replacements = self
            .replacements
            .iter()
            .enumerate()
            .filter_map(|(index, r)| {
                let window = r.precondition_window()?;
                let span = r.offset..r.offset.checked_add(r.len())?;
                Some(LabelledWindow {
                    owner: WindowOwner::Replacement(index),
                    window,
                    span: Some(span),
                })
            });
        let images = self
            .image_edits
            .iter()
            .enumerate()
            .filter_map(|(index, edit)| {
                Some(LabelledWindow {
                    owner: WindowOwner::ImageEdit(index),
                    window: edit.window()?,
                    span: None,
                })
            });
        replacements.chain(images).collect()
    }
}
