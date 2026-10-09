//! Reading a recipe file without trusting it, and checking a schema-v2 recipe before the input it
//! names is read.

use crate::input::read_pinned_input;
use anyhow::{Context, Result, bail};
use patch_core::xdj700::{
    ProtectedSet, RecipeError, RecipeTarget, check_recipe_against_protected_set, check_recipe_v2,
    recipe_target, unknown_release,
};
use patch_core::{RebuildError, open_regular_file, recipe_files};
use patch_schema::{RecipeV2, SCHEMA_VERSION_V2, SchemaVersionProbe, check_windows_across};
use std::io::Read;
use std::path::Path;

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

/// Largest protected-set file read: the measured set is about 30 KB.
pub const MAX_PROTECTED_SET_LEN: u64 = 8 * 1024 * 1024;

/// Reads a protected-set file only if it is a regular file of at most [`MAX_PROTECTED_SET_LEN`]
/// bytes of UTF-8 text.
pub fn read_protected_set(path: &Path) -> Result<String> {
    let read_failed = || format!("failed to read protected set '{}'", path.display());
    let file = open_regular_file(path).with_context(read_failed)?;
    let mut raw = Vec::new();
    // One byte more than the cap, so a larger file is refused without reading all of it.
    file.take(MAX_PROTECTED_SET_LEN + 1)
        .read_to_end(&mut raw)
        .with_context(read_failed)?;
    if raw.len() as u64 > MAX_PROTECTED_SET_LEN {
        bail!(
            "refusing protected set '{}': larger than {MAX_PROTECTED_SET_LEN} bytes",
            path.display()
        );
    }
    String::from_utf8(raw).with_context(|| {
        format!(
            "refusing protected set '{}': not UTF-8 text",
            path.display()
        )
    })
}

/// Prints the outcome of the protected-set check, or that none was given, so a run without the set
/// does not read like a checked one.
pub fn print_protected_set(set: Option<&ProtectedSet>) {
    match set {
        Some(set) => println!(
            "protected_set: {} ranges; no span or precondition window overlaps them",
            set.len()
        ),
        None => println!(
            "protected_set: not given (--protected-set); start-up and update-path code not checked"
        ),
    }
}

/// [`read_recipe`], then its `schema_version`, refusing any version other than 1 and 2 with the
/// same message for every command.
pub fn read_recipe_versioned(path: &Path) -> Result<(Vec<u8>, u32)> {
    let raw = read_recipe(path)?;
    let probe: SchemaVersionProbe = serde_json::from_slice(&raw)
        .with_context(|| format!("failed to parse recipe manifest JSON '{}'", path.display()))?;
    match probe.schema_version {
        1 | SCHEMA_VERSION_V2 => Ok((raw, probe.schema_version)),
        other => bail!(
            "refusing recipe '{}': unsupported schema_version {other}; supported: 1, 2",
            path.display()
        ),
    }
}

/// A schema-v2 recipe that passed every check that needs no firmware, with its release. The
/// fields are private, so the recipe cannot change after the checks. The library entry points
/// check it again on purpose: they do not trust their callers.
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

    /// Checks that the recipe's precondition windows are disjoint from those of every other
    /// recipe under `dir` for the same release (the committed recipes), so that no hash is
    /// computed for a window that would overlap one already published. `path` is the recipe's own
    /// file: if it lies under `dir`, it is skipped. A `dir` without another recipe for the
    /// release is refused, so a mistyped path cannot skip the check.
    pub fn check_against_committed(&self, path: &Path, dir: &Path) -> Result<()> {
        let committed_failed = || format!("failed to read committed recipes '{}'", dir.display());
        let own = std::fs::canonicalize(path)
            .with_context(|| format!("failed to resolve recipe '{}'", path.display()))?;
        let mut committed = Vec::new();
        for file in recipe_files(dir).with_context(committed_failed)? {
            if std::fs::canonicalize(&file).with_context(committed_failed)? == own {
                continue;
            }
            let raw = read_recipe(&file)?;
            let recipe: RecipeV2 = serde_json::from_slice(&raw).with_context(|| {
                format!("failed to parse committed recipe '{}'", file.display())
            })?;
            if recipe.target.release == self.recipe.target.release {
                committed.push(recipe);
            }
        }
        if committed.is_empty() {
            bail!(
                "refusing committed recipes '{}': it holds no other recipe for release {}; pass \
                 the repository's recipes directory",
                dir.display(),
                self.target.id
            );
        }
        check_windows_across(std::iter::once(&self.recipe).chain(&committed))
            .map_err(|error| anyhow::Error::new(error).context(self.refusing.clone()))
    }

    /// Reads the protected set at `path` (measured in emulation, kept outside the repository) and
    /// refuses the recipe if a replacement's span or precondition window overlaps it. Needs no
    /// firmware. Returns the set, for reporting.
    pub fn check_against_protected_set(&self, path: &Path) -> Result<ProtectedSet> {
        let text = read_protected_set(path)?;
        let set = ProtectedSet::parse(&text, self.target).with_context(|| {
            format!(
                "refusing protected set '{}' for release {}",
                path.display(),
                self.target.id
            )
        })?;
        check_recipe_against_protected_set(&self.recipe, self.target, &set)
            .map_err(|error| anyhow::Error::new(error).context(self.refusing.clone()))?;
        Ok(set)
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
