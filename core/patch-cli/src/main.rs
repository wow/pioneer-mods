use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use patch_core::identify_firmware;
use patch_schema::{PatchOperation, RecipeManifest};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "patch-cli",
    about = "Firmware patching CLI for owner-input Pioneer firmware workflows",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Inspect firmware identity (file name, size, SHA-256).
    Inspect(InspectArgs),
    /// Apply a patch recipe to a compatible firmware input.
    Patch(PatchArgs),
}

#[derive(clap::Args, Debug)]
struct InspectArgs {
    /// Path to the owner-supplied firmware file.
    #[arg(long)]
    input: PathBuf,

    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(clap::Args, Debug)]
struct PatchArgs {
    /// Path to the owner-supplied firmware file.
    #[arg(long)]
    input: PathBuf,

    /// Path to a recipe manifest.
    #[arg(long)]
    recipe: PathBuf,

    /// Path for generated patched firmware output.
    #[arg(long)]
    output: PathBuf,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run(cli)
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Inspect(args) => inspect(args),
        Commands::Patch(args) => patch(args),
    }
}

fn inspect(args: InspectArgs) -> Result<()> {
    let identity = identify_firmware(&args.input).with_context(|| {
        format!(
            "failed to inspect firmware identity for '{}'",
            args.input.display()
        )
    })?;

    match args.format {
        OutputFormat::Text => {
            println!("file_name: {}", identity.file_name);
            println!("size_bytes: {}", identity.size_bytes);
            println!("sha256_hex: {}", identity.sha256_hex);
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&identity)
                .context("failed to serialize inspection output as JSON")?;
            println!("{json}");
        }
    }

    Ok(())
}

fn patch(args: PatchArgs) -> Result<()> {
    let identity = identify_firmware(&args.input).with_context(|| {
        format!(
            "failed to inspect firmware identity for '{}'",
            args.input.display()
        )
    })?;

    let manifest = load_manifest(&args.recipe)?;
    manifest.validate().with_context(|| {
        format!(
            "recipe manifest validation failed for '{}'",
            args.recipe.display()
        )
    })?;

    let target = manifest
        .matching_target(identity.size_bytes, &identity.sha256_hex)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "input firmware '{}' (size={}, sha256={}) is not compatible with recipe '{}'",
                identity.file_name,
                identity.size_bytes,
                identity.sha256_hex,
                manifest.recipe_id
            )
        })?;

    let input_bytes = fs::read(&args.input)
        .with_context(|| format!("failed to read input firmware '{}'", args.input.display()))?;
    let patched_bytes = apply_operations(&input_bytes, &manifest.operations)?;

    fs::write(&args.output, patched_bytes).with_context(|| {
        format!(
            "failed to write output firmware '{}'",
            args.output.display()
        )
    })?;

    println!("recipe_id: {}", manifest.recipe_id);
    println!("matched_target: {} {}", target.model, target.version);
    println!("input_file: {}", identity.file_name);
    println!("output_file: {}", args.output.display());

    Ok(())
}

fn load_manifest(path: &Path) -> Result<RecipeManifest> {
    let raw = fs::read(path)
        .with_context(|| format!("failed to read recipe manifest '{}'", path.display()))?;
    serde_json::from_slice::<RecipeManifest>(&raw)
        .with_context(|| format!("failed to parse recipe manifest JSON '{}'", path.display()))
}

fn apply_operations(input_bytes: &[u8], operations: &[PatchOperation]) -> Result<Vec<u8>> {
    let mut output_bytes = input_bytes.to_vec();
    for (idx, operation) in operations.iter().enumerate() {
        match operation {
            PatchOperation::WriteSpan(span) => {
                let range = checked_range(
                    span.offset,
                    span.length,
                    output_bytes.len(),
                    &format!("operations[{idx}] write_span"),
                )?;
                if span.bytes.len() != range.len() {
                    bail!(
                        "operations[{idx}] write_span length mismatch: declared={}, bytes={}",
                        span.length,
                        span.bytes.len()
                    );
                }
                output_bytes[range].copy_from_slice(&span.bytes);
            }
            PatchOperation::OwnerCopyWindow(window) => {
                let source_range = checked_range(
                    window.source_offset,
                    window.length,
                    input_bytes.len(),
                    &format!("operations[{idx}] owner_copy_window source"),
                )?;
                let destination_range = checked_range(
                    window.destination_offset,
                    window.length,
                    output_bytes.len(),
                    &format!("operations[{idx}] owner_copy_window destination"),
                )?;
                output_bytes[destination_range].copy_from_slice(&input_bytes[source_range]);
            }
        }
    }
    Ok(output_bytes)
}

fn checked_range(
    offset: u64,
    length: u64,
    total_len: usize,
    context_label: &str,
) -> Result<std::ops::Range<usize>> {
    let start = usize::try_from(offset)
        .with_context(|| format!("{context_label}: offset does not fit platform usize"))?;
    let span_len = usize::try_from(length)
        .with_context(|| format!("{context_label}: length does not fit platform usize"))?;
    let end = start
        .checked_add(span_len)
        .ok_or_else(|| anyhow::anyhow!("{context_label}: offset+length overflowed"))?;
    if end > total_len {
        bail!(
            "{context_label}: out of bounds (offset={}, length={}, input_size={total_len})",
            offset,
            length
        );
    }
    Ok(start..end)
}
