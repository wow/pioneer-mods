use super::output::{Overwrite, ensure_safe_output_path, write_output_atomically};
use anyhow::{Context, Result};
use patch_core::{apply_recipe, firmware_file_name, read_regular_file};
use patch_schema::RecipeManifest;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(clap::Args, Debug)]
pub struct PatchArgs {
    /// Path to the owner-supplied firmware file.
    #[arg(long)]
    pub input: PathBuf,

    /// Path to a recipe manifest.
    #[arg(long)]
    pub recipe: PathBuf,

    /// Path for generated patched firmware output.
    #[arg(long)]
    pub output: PathBuf,

    /// Allow overwriting an existing output file.
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

pub fn patch(args: PatchArgs) -> Result<()> {
    let manifest = load_manifest(&args.recipe)?;
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

fn load_manifest(path: &Path) -> Result<RecipeManifest> {
    let raw = fs::read(path)
        .with_context(|| format!("failed to read recipe manifest '{}'", path.display()))?;
    serde_json::from_slice::<RecipeManifest>(&raw)
        .with_context(|| format!("failed to parse recipe manifest JSON '{}'", path.display()))
}
