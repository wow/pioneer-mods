//! Why a schema-v2 recipe was not applied: every refusal of the recipe entry points.

use crate::error::RebuildError;
use patch_schema::{
    MIN_IMAGE_DISTINCT_PIXELS, MIN_IMAGE_UNPREDICTED_PIXELS, MIN_PRECONDITION_LEN, RecipeV2Error,
};
use thiserror::Error;

/// Why a recipe was not applied.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RecipeError {
    #[error("recipe is invalid: {0}")]
    Invalid(#[from] RecipeV2Error),

    #[error("unknown release {release:?}; known: {known}")]
    UnknownRelease { release: String, known: String },

    #[error("{field} is {recipe}, but release {release} pins {pinned}")]
    TargetMismatch {
        field: &'static str,
        release: String,
        recipe: String,
        pinned: String,
    },

    #[error(
        "label {label} is not higher than the release's own version {stock}; the updater would \
         skip it"
    )]
    LabelNotHigher { label: String, stock: String },

    #[error(
        "replacements[{index}] {what} at {start:#x}..{end:#x} overlaps protected range \
         {protected_start:#x}..{protected_end:#x}"
    )]
    Protected {
        index: usize,
        what: &'static str,
        start: u64,
        end: u64,
        protected_start: usize,
        protected_end: usize,
    },

    /// Run-time addresses, inclusive and zero-padded to eight digits, as in the set's file.
    #[error(
        "replacements[{index}] {what} at run-time {start:#010x}..={last:#010x} overlaps the \
         protected set's range {set_start:#010x}..={set_last:#010x}: code that runs at start-up \
         or in the update path (docs/xdj700-flashing.md, section 5)"
    )]
    ProtectedSet {
        index: usize,
        what: &'static str,
        start: u64,
        last: u64,
        set_start: u64,
        set_last: u64,
    },

    #[error(
        "the protected set was measured on release {set}, not {target}; code moves between \
         releases"
    )]
    ProtectedSetRelease { set: String, target: String },

    /// Every rebuild writes the version string, so a set that covers it refuses every recipe.
    #[error(
        "the version string every rebuild writes, run-time {start:#010x}..={last:#010x}, lies in \
         the protected set's range {set_start:#010x}..={set_last:#010x}; check the set"
    )]
    ProtectedSetVersion {
        start: u64,
        last: u64,
        set_start: u64,
        set_last: u64,
    },

    #[error(
        "replacements[{index}].precondition ends at {end:#x}, past the application's end at \
         {len:#x}"
    )]
    OutOfBounds { index: usize, end: u64, len: usize },

    #[error(
        "replacements[{index}]: the {top} most common byte values fill {count} of the {len} \
         precondition window bytes around the span; a window that predictable could be inverted \
         to recover the rest, so choose a window over code"
    )]
    PredictableWindow {
        index: usize,
        top: usize,
        count: usize,
        len: usize,
    },

    #[error(
        "replacements[{index}]: the span's first and last bytes must differ from stock (unchanged \
         bytes at its edges belong outside the span)"
    )]
    UnchangedSpanEdge { index: usize },

    #[error(
        "replacements[{index}]: {unchanged} of the span's {len} bytes equal stock, {longest_run} \
         in a row, and bytes_hex would publish them; at most half may, fewer than \
         {MIN_PRECONDITION_LEN} in a row, so split the span around unchanged bytes"
    )]
    UnchangedSpanBytes {
        index: usize,
        unchanged: usize,
        longest_run: usize,
        len: usize,
    },

    /// The window's actual hash is not reported: this check cannot see other recipes' windows,
    /// and an error is easily pasted somewhere public. `patch-cli precondition` computes hashes
    /// after also checking the committed recipes.
    #[error(
        "replacements[{index}]: the precondition window does not have SHA-256 {expected}; the \
         recipe does not match this application"
    )]
    Precondition { index: usize, expected: String },

    #[error("{field} is {actual}, but the recipe expects {expected}")]
    UnexpectedOutput {
        field: &'static str,
        expected: String,
        actual: String,
    },

    #[error("image_edits[{index}] ends at {end:#x}, past the application's end at {len:#x}")]
    ImageOutOfBounds { index: usize, end: u64, len: usize },

    #[error(
        "image_edits[{index}] image at {start:#x}..{end:#x} overlaps protected range \
         {protected_start:#x}..{protected_end:#x}"
    )]
    ImageProtected {
        index: usize,
        start: u64,
        end: u64,
        protected_start: usize,
        protected_end: usize,
    },

    /// Run-time addresses, inclusive, as in the set's file.
    #[error(
        "image_edits[{index}] image at run-time {start:#010x}..={last:#010x} overlaps the \
         protected set's range {set_start:#010x}..={set_last:#010x}: code that runs at start-up \
         or in the update path (docs/xdj700-flashing.md, section 5)"
    )]
    ImageProtectedSet {
        index: usize,
        start: u64,
        last: u64,
        set_start: u64,
        set_last: u64,
    },

    /// The image's actual hash is not reported, as for replacement windows.
    #[error(
        "image_edits[{index}]: the stock image does not have SHA-256 {expected}; the recipe does \
         not match this application"
    )]
    ImagePrecondition { index: usize, expected: String },

    #[error(
        "image_edits[{index}]: the stock image holds {distinct} distinct pixel values (at least \
         {MIN_IMAGE_DISTINCT_PIXELS} needed) and {unpredicted} pixels its neighbours do not \
         predict (at least {MIN_IMAGE_UNPREDICTED_PIXELS} needed); the hash of an image that \
         predictable could be inverted, so choose an image with detail such as text"
    )]
    PredictableImage {
        index: usize,
        distinct: usize,
        unpredicted: usize,
    },

    #[error(transparent)]
    Rebuild(#[from] RebuildError),
}
