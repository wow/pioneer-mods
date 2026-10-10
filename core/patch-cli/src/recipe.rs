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
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Largest recipe file read: recipes are a few KB, so anything near this is not a recipe.
pub const MAX_RECIPE_LEN: u64 = 1024 * 1024;

/// Reads the recipe only if it is a regular file of at most [`MAX_RECIPE_LEN`] bytes, so a device,
/// a FIFO or a huge file is refused instead of read.
pub fn read_recipe(path: &Path) -> Result<Vec<u8>> {
    read_capped(
        path,
        MAX_RECIPE_LEN,
        "recipe manifest",
        "recipe",
        "any recipe",
    )
}

/// Reads `path` only if it is a regular file of at most `cap` bytes, checked before reading and
/// again after, so a file that grows while it is read is refused too. `what` names the file in a
/// read error, `refused` in a refusal, and `limit` what the cap stands for.
fn read_capped(path: &Path, cap: u64, what: &str, refused: &str, limit: &str) -> Result<Vec<u8>> {
    let read_failed = || format!("failed to read {what} '{}'", path.display());
    let file = open_regular_file(path).with_context(read_failed)?;
    let len = file.metadata().with_context(read_failed)?.len();
    if len > cap {
        bail!(
            "refusing {refused} '{}': {len} bytes is larger than {limit} (at most {cap})",
            path.display()
        );
    }
    let mut raw = Vec::with_capacity(len as usize);
    // One byte more than the cap, so a file that grew after the check is still caught.
    file.take(cap + 1)
        .read_to_end(&mut raw)
        .with_context(read_failed)?;
    if raw.len() as u64 > cap {
        bail!(
            "refusing {refused} '{}': it grew while being read",
            path.display()
        );
    }
    Ok(raw)
}

/// Largest protected-set file read: the measured set is about 30 KB.
pub const MAX_PROTECTED_SET_LEN: u64 = 8 * 1024 * 1024;

/// The environment variable that names the protected set when `--protected-set` is not given.
pub const PROTECTED_SET_ENV: &str = "XDJ700_PROTECTED_SET";

/// Reads a protected-set file only if it is a regular file of at most [`MAX_PROTECTED_SET_LEN`]
/// bytes of UTF-8 text.
pub fn read_protected_set(path: &Path) -> Result<String> {
    let raw = read_capped(
        path,
        MAX_PROTECTED_SET_LEN,
        "protected set",
        "protected set",
        "a protected set",
    )?;
    String::from_utf8(raw).with_context(|| {
        format!(
            "refusing protected set '{}': not UTF-8 text",
            path.display()
        )
    })
}

/// Where the protected set comes from. Skipping the check is a decision, never an omission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtectedSetSource {
    /// `--protected-set <file>`.
    Flag(PathBuf),
    /// The file named by [`PROTECTED_SET_ENV`].
    Env(PathBuf),
    /// `--no-protected-set`: the check is skipped on purpose.
    Skipped,
}

impl ProtectedSetSource {
    /// The source from the command's flags and, without them, the environment (`env`, normally
    /// the value of [`PROTECTED_SET_ENV`]). `--no-protected-set` wins over the environment; clap
    /// refuses it together with `--protected-set`.
    ///
    /// # Errors
    ///
    /// Neither flag and no environment variable: the check would be skipped silently.
    pub fn choose(flag: Option<&Path>, skip: bool, env: Option<OsString>) -> Result<Self> {
        match (flag, skip, env) {
            (Some(path), _, _) => Ok(Self::Flag(path.to_owned())),
            (None, true, _) => Ok(Self::Skipped),
            (None, false, Some(path)) if !path.is_empty() => Ok(Self::Env(PathBuf::from(path))),
            (None, false, _) => bail!(
                "no protected set: pass --protected-set <file> (or set {PROTECTED_SET_ENV}) to \
                 check the recipe against the code that runs at start-up and in the update path, \
                 or --no-protected-set to skip that check deliberately (docs/recipes.md, \
                 \"The protected set\")"
            ),
        }
    }
}

/// The report line for the protected-set check, so a run without the set does not read like a
/// checked one. Both commands print it on success.
pub fn protected_set_line(source: &ProtectedSetSource, set: Option<&ProtectedSet>) -> String {
    match (source, set) {
        (ProtectedSetSource::Skipped, _) | (_, None) => {
            "protected_set: skipped (--no-protected-set); start-up and update-path code not \
             checked"
                .to_owned()
        }
        (source, Some(set)) => format!(
            "protected_set: {} {}{}; no span, precondition window or edited image overlaps {}",
            set.len(),
            if set.len() == 1 { "range" } else { "ranges" },
            if matches!(source, ProtectedSetSource::Env(_)) {
                format!(" (from {PROTECTED_SET_ENV})")
            } else {
                String::new()
            },
            if set.len() == 1 { "it" } else { "them" }
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
    /// recipe under `dir` for the same release (the committed recipes), or repeat one of their
    /// replacements exactly, so that no hash is computed for a window that would overlap one
    /// already published. `path` is the recipe's own file: if it lies under `dir`, it is skipped,
    /// and so is any recipe with the same `recipe_id` (a copy of the draft, or the version it
    /// revises), which would otherwise count as an exact repeat of itself. A `dir` without
    /// another recipe for the release is refused, so a mistyped path cannot skip the check.
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
            if recipe.target.release == self.recipe.target.release
                && recipe.recipe_id != self.recipe.recipe_id
            {
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

    /// Reads the protected set `source` names (measured in emulation, kept outside the repository)
    /// and refuses the recipe if the set belongs to another release, covers the version string, or
    /// overlaps a replacement's span or precondition window. Needs no firmware. Returns the set,
    /// or `None` when the check is skipped on purpose (a warning goes to stderr).
    pub fn check_protected_set(&self, source: &ProtectedSetSource) -> Result<Option<ProtectedSet>> {
        let path = match source {
            ProtectedSetSource::Flag(path) | ProtectedSetSource::Env(path) => path,
            ProtectedSetSource::Skipped => {
                eprintln!(
                    "warning: --no-protected-set: recipe '{}' is not checked against the code \
                     that runs at start-up and in the update path",
                    self.recipe.recipe_id
                );
                return Ok(None);
            }
        };
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
        Ok(Some(set))
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
