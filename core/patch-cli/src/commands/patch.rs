use anyhow::{Context, Result, bail};
use patch_cli::input::read_pinned_input;
use patch_cli::output::{Overwrite, ensure_safe_output_path, write_output_atomically};
use patch_core::xdj700::{RecipeError, apply_recipe_v2, check_recipe_v2, recipe_target};
use patch_core::{RebuildError, apply_recipe, firmware_file_name, read_regular_file};
use patch_schema::{RecipeManifest, RecipeV2, SCHEMA_VERSION_V2, SchemaVersionProbe};
use std::fs;
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
}

pub fn patch(args: PatchArgs) -> Result<()> {
    let raw = fs::read(&args.recipe)
        .with_context(|| format!("failed to read recipe manifest '{}'", args.recipe.display()))?;
    let probe: SchemaVersionProbe = serde_json::from_slice(&raw).with_context(|| {
        format!(
            "failed to parse recipe manifest JSON '{}'",
            args.recipe.display()
        )
    })?;
    if probe.schema_version == SCHEMA_VERSION_V2 {
        patch_v2(&args, &raw)
    } else {
        patch_v1(&args, &raw)
    }
}

/// A schema-v2 recipe: the decoded application of a pinned release, rebuilt into a complete
/// update that is verified before it is written.
fn patch_v2(args: &PatchArgs, raw: &[u8]) -> Result<()> {
    let recipe: RecipeV2 = serde_json::from_slice(raw).with_context(|| {
        format!(
            "failed to parse schema-v2 recipe JSON '{}'",
            args.recipe.display()
        )
    })?;
    let refuse = |error: RecipeError| {
        anyhow::Error::new(error).context(format!(
            "refusing to apply recipe '{}' to '{}'",
            args.recipe.display(),
            args.input.display()
        ))
    };
    recipe.validate().map_err(|error| refuse(error.into()))?;
    if args.force {
        bail!(
            "--force is not accepted with a schema-v2 recipe: its output is an installable update \
             and is never overwritten; choose a new output path"
        );
    }
    let target = recipe_target(&recipe.target.release)
        .ok_or_else(|| refuse(RecipeError::UnknownRelease(recipe.target.release.clone())))?;
    // Every check that needs no firmware, before the input is read.
    check_recipe_v2(&recipe, target).map_err(refuse)?;
    ensure_safe_output_path(&args.input, &args.output, Overwrite::Never)?;
    let what = format!("the official update of release {}", target.id);
    let input = read_pinned_input(&args.input, target.release.upd_len, &what, "patch")?;

    let rebuilt = apply_recipe_v2(&recipe, &input).map_err(|error| match error {
        RecipeError::Rebuild(RebuildError::UnpinnedInput { sha256 }) => anyhow::anyhow!(
            "refusing to patch '{}': it is not {what} (SHA-256 {sha256}); only that exact file \
             is accepted",
            args.input.display()
        ),
        other => refuse(other),
    })?;
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
