//! Output-file safety shared by the commands that write firmware: never the input path, no
//! overwrite unless forced, atomic persist with fsync, and a read-back check.

use anyhow::{Context, Result, bail};
use patch_core::{read_regular_file, sha256_hex};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

/// Whether an existing output file may be replaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Overwrite {
    /// The user passed `--force`.
    Forced,
    /// Refuse; the command offers `--force`.
    RefuseUnlessForced,
    /// Refuse; the command never overwrites.
    Never,
}

impl Overwrite {
    pub(crate) fn from_force(force: bool) -> Self {
        if force {
            Self::Forced
        } else {
            Self::RefuseUnlessForced
        }
    }

    fn refusal(self, output_path: &Path) -> anyhow::Error {
        let hint = match self {
            Self::Never => "choose a new output path",
            Self::Forced | Self::RefuseUnlessForced => "pass --force to overwrite",
        };
        anyhow::anyhow!(
            "refusing to overwrite existing output file '{}'; {hint}",
            output_path.display()
        )
    }
}

pub(crate) fn ensure_safe_output_path(
    input_path: &Path,
    output_path: &Path,
    overwrite: Overwrite,
) -> Result<()> {
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

    if output_path.exists() && overwrite != Overwrite::Forced {
        return Err(overwrite.refusal(output_path));
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

pub(crate) fn write_output_atomically(
    output_path: &Path,
    bytes: &[u8],
    overwrite: Overwrite,
) -> Result<()> {
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
    let persist_result = if overwrite == Overwrite::Forced {
        temp_file.persist(output_path)
    } else {
        temp_file.persist_noclobber(output_path)
    };
    persist_result.map_err(|error| {
        if overwrite != Overwrite::Forced && error.error.kind() == std::io::ErrorKind::AlreadyExists
        {
            overwrite.refusal(output_path)
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

/// Reads `output_path` back and requires exactly `expected`, so what reached the disk is what was
/// verified in memory. Returns the SHA-256 of the bytes read.
pub(crate) fn verify_read_back(output_path: &Path, expected: &[u8]) -> Result<String> {
    let written = read_regular_file(output_path).with_context(|| {
        format!(
            "failed to read back output firmware '{}'",
            output_path.display()
        )
    })?;
    if written != expected {
        // Never leave an unverified firmware file behind under the requested name.
        let removed = fs::remove_file(output_path).is_ok();
        bail!(
            "output firmware '{}' does not match the verified bytes after writing ({}); do not use \
             it",
            output_path.display(),
            if removed {
                "removed"
            } else {
                "could not be removed"
            }
        );
    }
    Ok(sha256_hex(&written))
}
