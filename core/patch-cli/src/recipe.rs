//! Reading a recipe file without trusting it, and checking a schema-v2 recipe before the input it
//! names is read.

use anyhow::{Context, Result, bail};
use patch_core::xdj700::{
    RecipeError, RecipeTarget, check_recipe_v2, recipe_target, unknown_release,
};
use patch_core::{RebuildError, open_regular_file};
use patch_schema::RecipeV2;
use std::io::Read;
use std::path::Path;

use crate::input::read_pinned_input;

/// Largest recipe file read: recipes are a few KB, so anything near this is not a recipe.
pub const MAX_RECIPE_LEN: u64 = 1024 * 1024;

/// Reads the recipe only if it is a regular file of at most [`MAX_RECIPE_LEN`] bytes, so a device,
/// a FIFO or a huge file is refused instead of read.
pub fn read_recipe(path: &Path) -> Result<Vec<u8>> {
    let read_failed = || format!("failed to read recipe manifest '{}'", path.display());
    let file = open_regular_file(path).with_context(read_failed)?;
    let len = file.metadata().with_context(read_failed)?.len();
    if len > MAX_RECIPE_LEN {
        bail!(
            "refusing recipe '{}': {len} bytes is larger than any recipe (at most \
             {MAX_RECIPE_LEN})",
            path.display()
        );
    }
    let mut raw = Vec::with_capacity(len as usize);
    // One byte more than the cap, so a file that grew after the check is still caught.
    file.take(MAX_RECIPE_LEN + 1)
        .read_to_end(&mut raw)
        .with_context(read_failed)?;
    if raw.len() as u64 > MAX_RECIPE_LEN {
        bail!(
            "refusing recipe '{}': it grew while being read",
            path.display()
        );
    }
    Ok(raw)
}

/// A schema-v2 recipe that passed every check that needs no firmware, with its release. The
/// fields are private, so the recipe cannot change after the checks.
pub struct CheckedRecipe {
    recipe: RecipeV2,
    target: &'static RecipeTarget<'static>,
    /// Why a refusal happened, for example "refusing to apply recipe 'r.json' to 'XDJ700.UPD'".
    refusing: String,
}

impl CheckedRecipe {
    /// Parses `raw`, read from `path`, finds its release and runs `check_recipe_v2`. Refusals are
    /// reported under `refusing`.
    pub fn load(path: &Path, raw: &[u8], refusing: String) -> Result<Self> {
        let recipe: RecipeV2 = serde_json::from_slice(raw).with_context(|| {
            format!("failed to parse schema-v2 recipe JSON '{}'", path.display())
        })?;
        let refuse = |error| anyhow::Error::new(error).context(refusing.clone());
        let target = recipe_target(&recipe.target.release)
            .ok_or_else(|| refuse(unknown_release(&recipe.target.release)))?;
        check_recipe_v2(&recipe, target).map_err(refuse)?;
        Ok(Self {
            recipe,
            target,
            refusing,
        })
    }

    pub fn recipe(&self) -> &RecipeV2 {
        &self.recipe
    }

    pub fn target(&self) -> &'static RecipeTarget<'static> {
        self.target
    }

    /// Reads the official update of the recipe's release from `input`, refusing a file of the
    /// wrong length before reading it. `verb` names the refused action.
    pub fn read_input(&self, input: &Path, verb: &str) -> Result<Vec<u8>> {
        read_pinned_input(input, self.target.release.upd_len, &self.what(), verb)
    }

    /// An engine refusal: an input that is not the pinned file is reported as such, anything else
    /// under the refusal context.
    pub fn refusal(&self, input: &Path, verb: &str, error: RecipeError) -> anyhow::Error {
        match error {
            RecipeError::Rebuild(RebuildError::UnpinnedInput { sha256 }) => anyhow::anyhow!(
                "refusing to {verb} '{}': it is not {} (SHA-256 {sha256}); only that exact file \
                 is accepted",
                input.display(),
                self.what()
            ),
            other => anyhow::Error::new(other).context(self.refusing.clone()),
        }
    }

    fn what(&self) -> String {
        format!("the official update of release {}", self.target.id)
    }
}
