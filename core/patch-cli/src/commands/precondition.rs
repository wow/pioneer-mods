use anyhow::{Result, bail};
use patch_cli::recipe::{CheckedRecipe, protected_set_line, read_recipe_versioned};
use patch_core::firmware_file_name;
use patch_core::xdj700::precondition_hashes;
use patch_schema::SCHEMA_VERSION_V2;
use std::path::PathBuf;

#[derive(clap::Args, Debug)]
pub struct PreconditionArgs {
    /// Path to the official update of the recipe's release (owner-supplied).
    #[arg(long)]
    pub input: PathBuf,

    /// Path to a schema-v2 recipe, typically a draft whose precondition hashes are placeholders
    /// (any 64 hex digits). Every other field must already be valid.
    #[arg(long)]
    pub recipe: PathBuf,

    /// The repository's `recipes` directory itself, not a copy or a subset: the check is only as
    /// complete as this directory. The recipe's precondition windows must be disjoint from those
    /// of its other recipes for the release (the recipe's own file is skipped), checked before any
    /// hash is computed. CI checks the repository's directory again.
    #[arg(long)]
    pub committed_recipes: PathBuf,

    /// Exit with an error unless every hash the recipe declares matches, to confirm a completed
    /// recipe.
    #[arg(long, default_value_t = false)]
    pub check: bool,

    /// A protected set: run-time address ranges of the code that runs at start-up or in the update
    /// path, measured in emulation and kept outside the repository (format in docs/recipes.md).
    /// A recipe whose span or precondition window overlaps it is refused before any hash is
    /// computed.
    #[arg(long)]
    pub protected_set: Option<PathBuf>,
}

/// Prints the SHA-256 of every precondition window of a schema-v2 recipe. Each is computed on the
/// official update only after the recipe's checks, the check against the committed recipes, and
/// the window's bounds and leak checks. It writes nothing.
pub fn precondition(args: PreconditionArgs) -> Result<()> {
    let (raw, schema_version) = read_recipe_versioned(&args.recipe)?;
    if schema_version != SCHEMA_VERSION_V2 {
        bail!(
            "refusing recipe '{}': schema_version {schema_version} manifests have no \
             preconditions to hash; only schema_version {SCHEMA_VERSION_V2} recipes do",
            args.recipe.display()
        );
    }
    let refusing = format!(
        "refusing to hash the preconditions of recipe '{}'",
        args.recipe.display()
    );
    // Every check that needs no firmware, before the input is read.
    let checked = CheckedRecipe::load(&args.recipe, &raw, refusing)?;
    checked.check_against_committed(&args.recipe, &args.committed_recipes)?;
    let protected_set = match &args.protected_set {
        Some(path) => Some(checked.check_against_protected_set(path)?),
        None => None,
    };
    let (recipe, target) = (checked.recipe(), checked.target());
    let input = checked.read_input(&args.input, "hash preconditions on")?;
    let hashes = precondition_hashes(recipe, target, &input)
        .map_err(|error| checked.refusal(&args.input, "hash preconditions on", error))?;

    println!("recipe_id: {}", recipe.recipe_id);
    println!("release: {}", target.id);
    println!("input_file: {}", firmware_file_name(&args.input));
    println!("input_sha256_hex: {}", target.release.upd_sha256);
    println!("replacements: {}", hashes.len());
    let mut differing = 0;
    for (index, (replacement, sha256)) in recipe.replacements.iter().zip(&hashes).enumerate() {
        let window = replacement.precondition_window().expect("a checked recipe");
        let status = if replacement.precondition.sha256.eq_ignore_ascii_case(sha256) {
            "as declared"
        } else {
            differing += 1;
            "the recipe declares another hash"
        };
        println!(
            "replacements[{index}].precondition: {:#x}..{:#x} sha256 {sha256} ({status})",
            window.start, window.end
        );
    }
    println!("{}", protected_set_line(protected_set.as_ref()));
    println!(
        "checked: recipe, release pins, bounds, protected ranges, committed recipes' windows and \
         leak checks; each hash covers whatever is at its declared offset, so check the offsets \
         against your own analysis"
    );
    if args.check && differing > 0 {
        bail!(
            "{differing} of {} declared precondition hashes differ from the official update",
            hashes.len()
        );
    }
    Ok(())
}
