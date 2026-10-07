use anyhow::{Context, Result, bail};
use patch_core::{apply_recipe, read_firmware};
use patch_schema::RecipeManifest;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

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

    let (input_identity, input_bytes) = read_firmware(&args.input)
        .with_context(|| format!("failed to read input firmware '{}'", args.input.display()))?;
    let outcome = apply_recipe(&manifest, &input_bytes).with_context(|| {
        format!(
            "refusing to patch input firmware '{}' with recipe '{}'",
            args.input.display(),
            args.recipe.display()
        )
    })?;

    ensure_safe_output_path(&args.input, &args.output, args.force)?;
    write_output_atomically(&args.output, &outcome.output_bytes, args.force)?;

    println!("recipe_id: {}", manifest.recipe_id);
    println!(
        "matched_target: {} {}",
        outcome.target.model, outcome.target.version
    );
    println!("input_file: {}", input_identity.file_name);
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

fn ensure_safe_output_path(input_path: &Path, output_path: &Path, force: bool) -> Result<()> {
    let input_canonical = fs::canonicalize(input_path).with_context(|| {
        format!(
            "failed to canonicalize input path '{}'",
            input_path.display()
        )
    })?;
    let output_resolved = resolve_for_comparison(output_path)?;

    if output_resolved == input_canonical {
        bail!(
            "refusing to write output to the same path as input firmware ('{}')",
            input_canonical.display()
        );
    }

    if output_path.exists() && !force {
        bail!(
            "refusing to overwrite existing output file '{}'; pass --force to overwrite",
            output_path.display()
        );
    }

    Ok(())
}

fn resolve_for_comparison(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return fs::canonicalize(path)
            .with_context(|| format!("failed to canonicalize path '{}'", path.display()));
    }
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(std::env::current_dir()
        .context("failed to obtain current directory while resolving output path")?
        .join(path))
}

fn write_output_atomically(output_path: &Path, bytes: &[u8], force: bool) -> Result<()> {
    let output_dir = match output_path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    let mut temp_file = NamedTempFile::new_in(output_dir).with_context(|| {
        format!(
            "failed to create temporary output file in '{}'",
            output_dir.display()
        )
    })?;
    temp_file.write_all(bytes).with_context(|| {
        format!(
            "failed to write temporary output file for '{}'",
            output_path.display()
        )
    })?;
    temp_file.as_file().sync_all().with_context(|| {
        format!(
            "failed to sync temporary output file for '{}'",
            output_path.display()
        )
    })?;
    let persist_result = if force {
        temp_file.persist(output_path)
    } else {
        temp_file.persist_noclobber(output_path)
    };
    persist_result.map_err(|error| {
        if !force && error.error.kind() == std::io::ErrorKind::AlreadyExists {
            anyhow::anyhow!(
                "refusing to overwrite existing output file '{}'; pass --force to overwrite",
                output_path.display()
            )
        } else {
            anyhow::anyhow!(
                "failed to atomically persist output firmware '{}': {}",
                output_path.display(),
                error.error
            )
        }
    })?;
    sync_output_directory(output_dir)?;
    Ok(())
}

#[cfg(unix)]
fn sync_output_directory(output_dir: &Path) -> Result<()> {
    let dir_handle = fs::File::open(output_dir).with_context(|| {
        format!(
            "failed to open output directory '{}' for sync",
            output_dir.display()
        )
    })?;
    dir_handle.sync_all().with_context(|| {
        format!(
            "failed to sync output directory '{}' after atomic rename",
            output_dir.display()
        )
    })?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_output_directory(_output_dir: &Path) -> Result<()> {
    Ok(())
}
