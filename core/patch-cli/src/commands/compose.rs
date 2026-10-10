use anyhow::{Result, bail};
use patch_cli::output::{Overwrite, ensure_safe_output_path, write_output_atomically};
use patch_cli::recipe::{
    CheckedRecipe, PROTECTED_SET_ENV, ProtectedSetSource, protected_set_line, read_recipe_versioned,
};
use patch_core::firmware_file_name;
use patch_core::xdj700::{ComposeError, Composition, RecipeChecks, check_composition};
use patch_schema::{RecipeV2, SCHEMA_VERSION_V2};
use std::path::PathBuf;

#[derive(clap::Args, Debug)]
pub struct ComposeArgs {
    /// Path to the owner-supplied official update of the recipes' release.
    #[arg(long)]
    pub input: PathBuf,

    /// A schema-v2 recipe to compose; give it once per recipe (at least two). Each recipe must pin
    /// its output (`expected.application_sha256`): it is first applied alone and checked against
    /// its own output.
    #[arg(long = "recipe", required = true)]
    pub recipes: Vec<PathBuf>,

    /// MAIN version label of the output (`VerX.YY`), higher than the release's own version.
    #[arg(long)]
    pub label: String,

    /// Version the composed application reports (`X.YY`), lower than the release's own version.
    #[arg(long)]
    pub report_version: String,

    /// Path for the composed update.
    #[arg(long)]
    pub output: PathBuf,

    /// A protected set (format in docs/recipes.md); every recipe is checked against it before the
    /// input is read. Without it, `XDJ700_PROTECTED_SET` names the file.
    #[arg(long, conflicts_with = "no_protected_set")]
    pub protected_set: Option<PathBuf>,

    /// Skip the protected-set check on purpose (refused otherwise without a set).
    #[arg(long, default_value_t = false)]
    pub no_protected_set: bool,
}

/// Composes several schema-v2 recipes into one update with one label and one reported version,
/// checked against each recipe's own output (`docs/modular-builds.md`).
pub fn compose(args: ComposeArgs) -> Result<()> {
    if args.recipes.len() < 2 {
        bail!("compose needs at least two recipes; apply a single recipe with `patch`");
    }
    let refusing = format!("refusing to compose recipes for '{}'", args.input.display());
    // Every check that needs no firmware, for every recipe, before the input is read.
    let mut checked = Vec::with_capacity(args.recipes.len());
    for path in &args.recipes {
        let (raw, schema_version) = read_recipe_versioned(path)?;
        if schema_version != SCHEMA_VERSION_V2 {
            bail!(
                "{refusing}: '{}' is schema_version {schema_version}; only schema-v2 recipes \
                 can be composed",
                path.display()
            );
        }
        // Each recipe's refusals name it.
        let context = format!("{refusing}: recipe '{}'", path.display());
        checked.push(CheckedRecipe::load(path, &raw, context)?);
    }
    let target = checked[0].target();
    if let Some(other) = checked.iter().find(|c| c.target().id != target.id) {
        bail!(
            "{refusing}: recipes for releases {} and {} cannot be composed",
            target.id,
            other.target().id
        );
    }
    let source = ProtectedSetSource::choose(
        args.protected_set.as_deref(),
        args.no_protected_set,
        std::env::var_os(PROTECTED_SET_ENV),
    )?;
    // Read once, for the release every recipe names.
    let protected_set = source.load(target)?;
    for recipe in &checked {
        recipe.check_against_protected_set(protected_set.as_ref())?;
    }
    let fragments: Vec<RecipeV2> = checked.iter().map(|c| c.recipe().clone()).collect();
    let composition = Composition {
        label: &args.label,
        reported_version: &args.report_version,
    };
    let checks = RecipeChecks {
        protected_set: protected_set.as_ref(),
    };
    // Every refusal names the files of the recipes it is about.
    let refuse = |error: ComposeError| match error {
        ComposeError::Fragment { index, source, .. } => {
            checked[index].refusal(&args.input, "compose", source)
        }
        other => {
            let files: Vec<String> = other
                .recipes()
                .into_iter()
                .map(|index| format!("'{}'", args.recipes[index].display()))
                .collect();
            let context = match files.as_slice() {
                [] => refusing.clone(),
                [file] => format!("{refusing}: recipe {file}"),
                files => format!("{refusing}: recipes {}", files.join(" and ")),
            };
            anyhow::Error::new(other).context(context)
        }
    };
    // Distinct recipes, the pins, the windows and images across the recipes, and the label and
    // version.
    let composition = check_composition(&fragments, target, composition, checks).map_err(refuse)?;
    ensure_safe_output_path(&args.input, &args.output, Overwrite::Never)?;
    let input = checked[0].read_input(&args.input, "compose")?;

    let composed = composition.compose(&input).map_err(refuse)?;
    let rebuilt = &composed.rebuilt;
    write_output_atomically(&args.output, rebuilt.bytes(), Overwrite::Never)?;

    println!("release: {}", target.id);
    println!("input_file: {}", firmware_file_name(&args.input));
    println!("input_sha256_hex: {}", target.release.upd_sha256);
    for (index, (recipe, alone)) in fragments.iter().zip(&composed.fragments).enumerate() {
        let update_pinned = recipe
            .expected
            .as_ref()
            .is_some_and(|e| e.upd_sha256.is_some());
        println!(
            "recipe[{index}]: {} (alone: application {}, as pinned; update {}, {})",
            recipe.recipe_id,
            alone.application_sha256,
            alone.upd_sha256,
            if update_pinned {
                "as pinned"
            } else {
                "not pinned"
            }
        );
    }
    println!(
        "application_reported_version: {}",
        rebuilt.application_reported_version().unwrap_or("none")
    );
    println!("application_sha256_hex: {}", rebuilt.application_sha256());
    println!("version_label: {}", args.label);
    println!("main_image_len: {}", rebuilt.main_image_len());
    println!("main_image_sha256_hex: {}", rebuilt.main_image_sha256());
    println!("output_file: {}", args.output.display());
    println!("output_len: {}", rebuilt.bytes().len());
    println!("output_sha256_hex: {}", rebuilt.sha256());
    println!("{}", protected_set_line(&source, protected_set.as_ref()));
    println!(
        "verified: each recipe reproduced its own pinned output alone; the composed application \
         equals each recipe's output where it changes bytes and stock elsewhere; the rebuild was \
         re-parsed and checked against the input; file read back before it was renamed into place"
    );
    println!(
        "note: a combination is a new update with no pin of its own: rehearse it both ways in \
         emulation before flashing; unless it is a listed combination it is experimental at most \
         (docs/modular-builds.md)"
    );
    Ok(())
}
