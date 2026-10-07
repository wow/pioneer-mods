use anyhow::{Context, Result, bail};
use patch_core::identify_bytes;
use patch_schema::{PatchOperation, RecipeManifest};
use std::fs;
use std::io::Write;
use std::ops::Range;
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
    write_output_atomically(&args.output, &patched_bytes, args.force)?;

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

fn firmware_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
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
