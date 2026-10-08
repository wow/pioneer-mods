use anyhow::{Context, Result, bail};
use patch_cli::input::read_pinned_input;
use patch_cli::output::{Overwrite, ensure_safe_output_path, write_output_atomically};
use patch_core::xdj700::{
    RecipeError, apply_recipe_v2_to, check_recipe_v2, recipe_target, unknown_release,
};
use patch_core::{
    RebuildError, apply_recipe, firmware_file_name, open_regular_file, read_regular_file,
};
use patch_schema::{RecipeManifest, RecipeV2, SCHEMA_VERSION_V2, SchemaVersionProbe};
use std::io::Read;
use std::path::{Path, PathBuf};

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

/// Largest recipe file read: recipes are a few KB, so anything near this is not a recipe.
const MAX_RECIPE_LEN: u64 = 1024 * 1024;

pub fn patch(args: PatchArgs) -> Result<()> {
    let raw = read_recipe(&args.recipe)?;
    let probe: SchemaVersionProbe = serde_json::from_slice(&raw).with_context(|| {
        format!(
            "failed to parse recipe manifest JSON '{}'",
            args.recipe.display()
        )
    })?;
    match probe.schema_version {
        1 => patch_v1(&args, &raw),
        SCHEMA_VERSION_V2 => patch_v2(&args, &raw),
        other => bail!(
            "refusing recipe '{}': unsupported schema_version {other}; supported: 1, 2",
            args.recipe.display()
        ),
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
    let target = recipe_target(&recipe.target.release)
        .ok_or_else(|| refuse(unknown_release(&recipe.target.release)))?;
    // Every check that needs no firmware (it validates the recipe first), before the input is read.
    check_recipe_v2(&recipe, target).map_err(refuse)?;
    if args.force {
        bail!(
            "--force is not accepted with a schema-v2 recipe: its output is an installable update \
             and is never overwritten; choose a new output path"
        );
    }
    ensure_safe_output_path(&args.input, &args.output, Overwrite::Never)?;
    let what = format!("the official update of release {}", target.id);
    let input = read_pinned_input(&args.input, target.release.upd_len, &what, "patch")?;

    let rebuilt = apply_recipe_v2_to(&recipe, target, &input).map_err(|error| match error {
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

/// Reads the recipe only if it is a regular file of at most [`MAX_RECIPE_LEN`] bytes, so a device,
/// a FIFO or a huge file is refused instead of read.
fn read_recipe(path: &Path) -> Result<Vec<u8>> {
    let read_failed = || format!("failed to read recipe manifest '{}'", path.display());
    let file = open_regular_file(path).with_context(read_failed)?;
    let len = file.metadata().with_context(read_failed)?.len();
    if len > MAX_RECIPE_LEN {
        bail!(
            "refusing recipe '{}': {len} bytes is larger than any recipe (at most {MAX_RECIPE_LEN})",
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
