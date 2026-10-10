//! Why composing recipes into one build was refused ([`super::compose_recipes`]).

use super::recipe_error::RecipeError;
use crate::error::{SectionError, UpdError};
use patch_schema::WindowOverlap;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ComposeError {
    #[error("nothing to compose: no recipe was given")]
    NoFragments,

    /// The same recipe given twice, or two recipes sharing a `recipe_id`.
    #[error(
        "recipes {first} and {second} are both {recipe_id}; give each recipe once (recipe ids \
         must differ)"
    )]
    Duplicate {
        first: usize,
        second: usize,
        recipe_id: String,
    },

    /// A recipe failed its own checks, or did not reproduce its own pinned output when applied
    /// alone.
    #[error("recipe {index} ({recipe_id}): {source}")]
    Fragment {
        index: usize,
        recipe_id: String,
        #[source]
        source: RecipeError,
    },

    #[error(
        "recipe {index} ({recipe_id}) must pin expected.application_sha256 to be composed: each \
         recipe is first checked against its own output"
    )]
    Unpinned { index: usize, recipe_id: String },

    #[error(transparent)]
    Overlap(#[from] WindowOverlap),

    #[error(
        "{first} and {second} edit overlapping images; recipes in one build must edit different \
         images, or make the same edit"
    )]
    ImageOverlap {
        first: String,
        /// The position of the recipe holding `first`.
        first_recipe: usize,
        second: String,
        second_recipe: usize,
    },

    /// The composition's label or reported version, or the composed rebuild, was refused.
    #[error("the composed build: {0}")]
    Composition(#[source] RecipeError),

    #[error("{first} and {second} write different bytes at decoded offset {offset:#x}")]
    Disagree {
        offset: usize,
        first: String,
        second: String,
    },

    /// The composed application is not what its recipes produce alone: `expected` names the
    /// recipe whose output it should match at that byte, or "stock".
    #[error("the composed application differs from {expected} at decoded offset {offset:#x}")]
    Mismatch { offset: usize, expected: String },

    #[error("a rebuilt update could not be parsed: {0}")]
    Parse(#[from] UpdError),

    #[error("a rebuilt update's application could not be decoded: {0}")]
    Decode(#[from] SectionError),
}

impl ComposeError {
    /// The positions of the recipes this refusal is about, in the order they were given, so that
    /// a front end can name their files. Empty for a refusal of the composition as a whole.
    pub fn recipes(&self) -> Vec<usize> {
        match self {
            Self::Fragment { index, .. } | Self::Unpinned { index, .. } => vec![*index],
            Self::Duplicate { first, second, .. } => vec![*first, *second],
            Self::Overlap(overlap) => vec![overlap.first_recipe, overlap.second_recipe],
            Self::ImageOverlap {
                first_recipe,
                second_recipe,
                ..
            } => vec![*first_recipe, *second_recipe],
            _ => Vec::new(),
        }
    }
}
