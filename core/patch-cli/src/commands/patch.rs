use anyhow::{Context, Result, bail};
use patch_cli::output::{Overwrite, ensure_safe_output_path, write_output_atomically};
use patch_cli::recipe::{CheckedRecipe, protected_set_line, read_recipe_versioned};
use patch_core::xdj700::apply_recipe_v2_to;
use patch_core::{apply_recipe, firmware_file_name, read_regular_file};
use patch_schema::{RecipeManifest, SCHEMA_VERSION_V2};
use std::path::PathBuf;

#[derive(clap::Args, Debug)]
pub struct PatchArgs {
    /// Path to the owner-supplied firmware file.
    #[arg(long)]
    pub input: PathBuf,

    /// Path to a recipe manifest (JSON). `schema_version` 2 recipes change the decoded application
    /// of a pinned release and write a complete, verified update; version 1 manifests write raw
    /// byte spans and cannot produce an installable update.
    #[arg(long)]
    pub recipe: PathBuf,

    /// Path for generated patched firmware output.
    #[arg(long)]
    pub output: PathBuf,

    /// Allow overwriting an existing output file (schema v1 only; a v2 output is never
    /// overwritten).
    #[arg(long, default_value_t = false)]
    pub force: bool,

    /// A protected set: run-time address ranges of the code that runs at start-up or in the update
    /// path, measured in emulation and kept outside the repository (format in docs/recipes.md).
    /// A schema-v2 recipe whose span or precondition window overlaps it is refused before the
    /// input is read.
    #[arg(long)]
    pub protected_set: Option<PathBuf>,
}

pub fn patch(args: PatchArgs) -> Result<()> {
    let (raw, schema_version) = read_recipe_versioned(&args.recipe)?;
    if schema_version == SCHEMA_VERSION_V2 {
        patch_v2(&args, &raw)
    } else if args.protected_set.is_some() {
        bail!(
            "--protected-set applies only to schema-v2 recipes; '{}' is schema_version \
             {schema_version}",
            args.recipe.display()
        )
    } else {
        patch_v1(&args, &raw)
    }
}

/// A schema-v2 recipe: the decoded application of a pinned release, rebuilt into a complete
/// update that is verified before it is written.
fn patch_v2(args: &PatchArgs, raw: &[u8]) -> Result<()> {
    let refusing = format!(
        "refusing to apply recipe '{}' to '{}'",
        args.recipe.display(),
        args.input.display()
    );
    // Every check that needs no firmware (it validates the recipe first), before the input is read.
    let checked = CheckedRecipe::load(&args.recipe, raw, refusing)?;
    let protected_set = match &args.protected_set {
        Some(path) => Some(checked.check_against_protected_set(path)?),
        None => None,
    };
    let (recipe, target) = (checked.recipe(), checked.target());
    if args.force {
        bail!(
            "--force is not accepted with a schema-v2 recipe: its output is an installable update \
             and is never overwritten; choose a new output path"
        );
    }
    ensure_safe_output_path(&args.input, &args.output, Overwrite::Never)?;
    let input = checked.read_input(&args.input, "patch")?;

    let rebuilt = apply_recipe_v2_to(recipe, target, &input)
        .map_err(|error| checked.refusal(&args.input, "patch", error))?;
    write_output_atomically(&args.output, rebuilt.bytes(), Overwrite::Never)?;

    println!("recipe_id: {}", recipe.recipe_id);
    println!("release: {}", target.id);
    println!("input_file: {}", firmware_file_name(&args.input));
    println!("input_sha256_hex: {}", target.release.upd_sha256);
    println!("replacements: {}", recipe.replacements.len());
    println!(
        "application_reported_version: {}",
        rebuilt.application_reported_version().unwrap_or("none")
    );
    println!("application_sha256_hex: {}", rebuilt.application_sha256());
    println!("version_label: {}", recipe.label);
    println!("main_image_len: {}", rebuilt.main_image_len());
    println!("main_image_sha256_hex: {}", rebuilt.main_image_sha256());
    println!("output_file: {}", args.output.display());
    println!("output_len: {}", rebuilt.bytes().len());
    println!("output_sha256_hex: {}", rebuilt.sha256());
    println!("{}", protected_set_line(protected_set.as_ref()));
    println!(
        "verified: preconditions, protected ranges and bounded diff checked; rebuild re-parsed \
         and checked against the input; file read back through the file system before it was \
         renamed into place"
    );
    Ok(())
}

/// A schema-v1 manifest: raw byte spans on the input file (not an installable update).
fn patch_v1(args: &PatchArgs, raw: &[u8]) -> Result<()> {
    let manifest = serde_json::from_slice::<RecipeManifest>(raw).with_context(|| {
        format!(
            "failed to parse recipe manifest JSON '{}'",
            args.recipe.display()
        )
    })?;
    manifest.validate().with_context(|| {
        format!(
            "recipe manifest validation failed for '{}'",
            args.recipe.display()
        )
    })?;

    // The engine hashes the bytes itself; reading without hashing avoids a second full pass.
    let input_bytes = read_regular_file(&args.input)
        .with_context(|| format!("failed to read input firmware '{}'", args.input.display()))?;
    let outcome = apply_recipe(&manifest, &input_bytes).with_context(|| {
        format!(
            "refusing to patch input firmware '{}' with recipe '{}'",
            args.input.display(),
            args.recipe.display()
        )
    })?;

    let overwrite = Overwrite::from_force(args.force);
    ensure_safe_output_path(&args.input, &args.output, overwrite)?;
    write_output_atomically(&args.output, &outcome.output_bytes, overwrite)?;

    println!("recipe_id: {}", manifest.recipe_id);
    println!(
        "matched_target: {} {}",
        outcome.target.model, outcome.target.version
    );
    println!("input_file: {}", firmware_file_name(&args.input));
    println!("input_sha256_hex: {}", outcome.input_sha256_hex);
    println!("output_file: {}", args.output.display());
    println!("output_sha256_hex: {}", outcome.output_sha256_hex);

    Ok(())
}
