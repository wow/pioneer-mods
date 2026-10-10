use anyhow::{Result, bail};
use patch_cli::recipe::{
    CheckedRecipe, PROTECTED_SET_ENV, ProtectedSetSource, protected_set_line, read_recipe_versioned,
};
use patch_core::firmware_file_name;
use patch_core::xdj700::{RecipeChecks, precondition_hashes};
use patch_schema::SCHEMA_VERSION_V2;
use std::path::PathBuf;

#[derive(clap::Args, Debug)]
pub struct PreconditionArgs {
    /// Path to the official update of the recipe's release (owner-supplied).
    #[arg(long)]
    pub input: PathBuf,

    /// Path to a schema-v2 recipe, typically a draft whose precondition hashes (and, with image
    /// edits, `expected.application_sha256`) are placeholders (any 64 hex digits). Every other
    /// field must already be valid.
    #[arg(long)]
    pub recipe: PathBuf,

    /// The repository's `recipes` directory itself, not a copy or a subset: the check is only as
    /// complete as this directory. The recipe's precondition windows must be disjoint from those
    /// of its other recipes for the release, or repeat one of their replacements exactly (the
    /// recipe's own file, and any recipe with its `recipe_id`, are skipped), checked before any
    /// hash is computed. CI checks the repository's directory again.
    #[arg(long)]
    pub committed_recipes: PathBuf,

    /// Exit with an error unless every hash the recipe declares matches, to confirm a completed
    /// recipe.
    #[arg(long, default_value_t = false)]
    pub check: bool,

    /// A protected set: run-time address ranges of the code that runs at start-up or in the update
    /// path, measured in emulation and kept outside the repository (format in docs/recipes.md).
    /// A recipe whose span, precondition window or edited image overlaps it is refused before any
    /// hash is computed. Without it, `XDJ700_PROTECTED_SET` names the file.
    #[arg(long, conflicts_with = "no_protected_set")]
    pub protected_set: Option<PathBuf>,

    /// Skip the protected-set check on purpose (a recipe without a set is refused otherwise).
    #[arg(long, default_value_t = false)]
    pub no_protected_set: bool,
}

/// Prints the SHA-256 of every precondition window of a schema-v2 recipe and the identities of its
/// output, from a rebuild of the recipe on the official update that runs only after the recipe's
/// checks and the check against the committed recipes, and hashes each window only after its
/// bounds and leak checks. It writes nothing.
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
    let source = ProtectedSetSource::choose(
        args.protected_set.as_deref(),
        args.no_protected_set,
        std::env::var_os(PROTECTED_SET_ENV),
    )?;
    let protected_set = checked.check_protected_set(&source)?;
    let (recipe, target) = (checked.recipe(), checked.target());
    let input = checked.read_input(&args.input, "hash preconditions on")?;
    // The engine runs every check again, the protected set included.
    let checks = RecipeChecks {
        protected_set: protected_set.as_ref(),
    };
    let hashes = precondition_hashes(recipe, target, &input, checks)
        .map_err(|error| checked.refusal(&args.input, "hash preconditions on", error))?;

    println!("recipe_id: {}", recipe.recipe_id);
    println!("release: {}", target.id);
    println!("input_file: {}", firmware_file_name(&args.input));
    println!("input_sha256_hex: {}", target.release.upd_sha256);
    println!("replacements: {}", hashes.replacements.len());
    println!("image_edits: {}", recipe.image_edits.len());
    let (mut windows, mut outputs) = (Tally::default(), Tally::default());
    for (index, (replacement, sha256)) in recipe
        .replacements
        .iter()
        .zip(&hashes.replacements)
        .enumerate()
    {
        let window = replacement.precondition_window().expect("a checked recipe");
        let differs = !replacement.precondition.sha256.eq_ignore_ascii_case(sha256);
        let status = windows.status(true, differs);
        println!(
            "replacements[{index}].precondition: {:#x}..{:#x} sha256 {sha256} ({status})",
            window.start, window.end
        );
    }
    for (index, edit) in recipe.image_edits.iter().enumerate() {
        let window = edit.window().expect("a checked recipe");
        println!(
            "image_edits[{index}]: {:#x}..{:#x} ({}x{}), no hash published",
            window.start, window.end, edit.width, edit.height
        );
    }
    for pin in hashes.output.pins(recipe.expected.as_ref()) {
        let status = outputs.status(pin.declared.is_some(), pin.differs());
        println!("expected.{}: {} ({status})", pin.field, pin.actual);
    }
    println!("{}", protected_set_line(&source, protected_set.as_ref()));
    println!(
        "checked: recipe, release pins, bounds, protected ranges, committed recipes' windows, \
         leak checks and the rebuild; each hash covers whatever is at its declared offset, so \
         check the offsets against your own analysis"
    );
    let failures: Vec<String> = [
        (
            windows,
            "precondition hashes differ from the official update",
        ),
        (outputs, "output identities differ from the rebuilt output"),
    ]
    .into_iter()
    .filter(|(tally, _)| tally.differing > 0)
    .map(|(tally, what)| format!("{} of {} declared {what}", tally.differing, tally.compared))
    .collect();
    if args.check && !failures.is_empty() {
        bail!("{}", failures.join("; "));
    }
    Ok(())
}

/// Declared values of one kind, compared with those computed.
#[derive(Debug, Default, Clone, Copy)]
struct Tally {
    compared: usize,
    differing: usize,
}

impl Tally {
    /// Counts one value and describes it: `declared` says whether the recipe declares it, and
    /// `differs` whether the declared value differs from the computed one.
    fn status(&mut self, declared: bool, differs: bool) -> &'static str {
        if !declared {
            return "not declared";
        }
        self.compared += 1;
        if differs {
            self.differing += 1;
            "the recipe declares another hash"
        } else {
            "as declared"
        }
    }
}
