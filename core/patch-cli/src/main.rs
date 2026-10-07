use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use patch_core::{identify_bytes, identify_firmware};
use patch_schema::{PatchOperation, RecipeManifest};
use std::fs;
use std::io::Write;
use std::ops::Range;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

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

    /// Allow overwriting an existing output file.
    #[arg(long, default_value_t = false)]
    force: bool,
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
    let manifest = load_manifest(&args.recipe)?;
    manifest.validate().with_context(|| {
        format!(
            "recipe manifest validation failed for '{}'",
            args.recipe.display()
        )
    })?;

    let input_bytes = fs::read(&args.input)
        .with_context(|| format!("failed to read input firmware '{}'", args.input.display()))?;
    let identity = identify_bytes(firmware_file_name(&args.input), &input_bytes);

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

    let declared_destination_ranges =
        declared_destination_ranges(&manifest.operations, input_bytes.len())?;
    let patched_bytes = apply_operations(&input_bytes, &manifest.operations)?;
    verify_mutations_within_declared_regions(
        &input_bytes,
        &patched_bytes,
        &declared_destination_ranges,
    )?;

    let output_identity = identify_bytes(firmware_file_name(&args.output), &patched_bytes);
    if let Some(expected_output_sha256) = &target.expected_output_sha256
        && !output_identity
            .sha256_hex
            .eq_ignore_ascii_case(expected_output_sha256)
    {
        bail!(
            "output SHA-256 mismatch for recipe '{}': expected={}, actual={}",
            manifest.recipe_id,
            expected_output_sha256,
            output_identity.sha256_hex
        );
    }

    ensure_safe_output_path(&args.input, &args.output, args.force)?;
    write_output_atomically(&args.output, &patched_bytes)?;

    println!("recipe_id: {}", manifest.recipe_id);
    println!("matched_target: {} {}", target.model, target.version);
    println!("input_file: {}", identity.file_name);
    println!("input_sha256_hex: {}", identity.sha256_hex);
    println!("output_file: {}", args.output.display());
    println!("output_sha256_hex: {}", output_identity.sha256_hex);

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

fn declared_destination_ranges(
    operations: &[PatchOperation],
    total_len: usize,
) -> Result<Vec<Range<usize>>> {
    let mut indexed_ranges: Vec<(usize, Range<usize>)> = Vec::with_capacity(operations.len());
    for (idx, operation) in operations.iter().enumerate() {
        let range = match operation {
            PatchOperation::WriteSpan(span) => checked_range(
                span.offset,
                span.length,
                total_len,
                &format!("operations[{idx}] write_span destination"),
            )?,
            PatchOperation::OwnerCopyWindow(window) => checked_range(
                window.destination_offset,
                window.length,
                total_len,
                &format!("operations[{idx}] owner_copy_window destination"),
            )?,
        };
        indexed_ranges.push((idx, range));
    }

    indexed_ranges.sort_by_key(|(_, range)| (range.start, range.end));
    for pair in indexed_ranges.windows(2) {
        let (previous_idx, previous_range) = &pair[0];
        let (current_idx, current_range) = &pair[1];
        if current_range.start < previous_range.end {
            bail!(
                "operations[{current_idx}] destination range {}..{} overlaps with operations[{previous_idx}] range {}..{}",
                current_range.start,
                current_range.end,
                previous_range.start,
                previous_range.end
            );
        }
    }

    Ok(indexed_ranges
        .into_iter()
        .map(|(_, range)| range)
        .collect::<Vec<_>>())
}

fn verify_mutations_within_declared_regions(
    input_bytes: &[u8],
    output_bytes: &[u8],
    declared_destination_ranges: &[Range<usize>],
) -> Result<()> {
    if input_bytes.len() != output_bytes.len() {
        bail!(
            "mutation verification failed: input and output lengths differ (input={}, output={})",
            input_bytes.len(),
            output_bytes.len()
        );
    }

    let mut range_idx = 0usize;
    for (byte_idx, (input_byte, output_byte)) in input_bytes.iter().zip(output_bytes).enumerate() {
        if input_byte == output_byte {
            continue;
        }

        while range_idx < declared_destination_ranges.len()
            && byte_idx >= declared_destination_ranges[range_idx].end
        {
            range_idx += 1;
        }

        if range_idx >= declared_destination_ranges.len()
            || byte_idx < declared_destination_ranges[range_idx].start
        {
            bail!(
                "mutation verification failed: byte offset {} changed outside declared destination ranges",
                byte_idx
            );
        }
    }

    Ok(())
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

fn write_output_atomically(output_path: &Path, bytes: &[u8]) -> Result<()> {
    let output_dir = output_path.parent().unwrap_or_else(|| Path::new("."));
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
    temp_file.persist(output_path).map_err(|error| {
        anyhow::anyhow!(
            "failed to atomically persist output firmware '{}': {}",
            output_path.display(),
            error.error
        )
    })?;
    Ok(())
}

fn firmware_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}
