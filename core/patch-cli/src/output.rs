//! Output-file safety shared by the commands that write firmware: never the input path, no
//! overwrite unless forced, and an atomic, synced write whose bytes are read back before they
//! appear under the output name.

use anyhow::{Context, Result, bail};
use patch_core::{read_regular_file, sha256_hex};
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

/// Whether an existing output file may be replaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overwrite {
    /// The user passed `--force`.
    Forced,
    /// Refuse; the command offers `--force`.
    RefuseUnlessForced,
    /// Refuse; the command never overwrites.
    Never,
}

impl Overwrite {
    pub fn from_force(force: bool) -> Self {
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

/// Refuses an output path that resolves to the input, or that exists unless overwriting is
/// forced.
///
/// # Errors
///
/// On aliasing, an existing output, or a path that cannot be resolved.
pub fn ensure_safe_output_path(
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

/// Writes `bytes` to `output_path` atomically and returns their SHA-256.
///
/// The bytes go to a temporary file in the same directory (never under the output name), which
/// is synced and then read back through the file system ([`read_back`]) before it is renamed into
/// place, so only bytes that read back exactly can appear under `output_path`. The temporary file
/// is deleted on any error. After the rename the directory is synced; if that fails, the output
/// is removed. A crash after the rename can only leave the complete, verified file, because the
/// rename is atomic.
///
/// # Errors
///
/// An overwrite refusal (see [`Overwrite`]), a write, sync or read-back failure, or a file system
/// without a no-clobber rename (for example exFAT), with a hint to write elsewhere first.
pub fn write_output_atomically(
    output_path: &Path,
    bytes: &[u8],
    overwrite: Overwrite,
) -> Result<String> {
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
    let sha256 = read_back(temp_file.path(), bytes).with_context(|| {
        format!(
            "refusing to create output firmware '{}'",
            output_path.display()
        )
    })?;
    let persist_result = if overwrite == Overwrite::Forced {
        temp_file.persist(output_path)
    } else {
        temp_file.persist_noclobber(output_path)
    };
    persist_result.map_err(|error| match error.error.kind() {
        ErrorKind::AlreadyExists if overwrite != Overwrite::Forced => {
            overwrite.refusal(output_path)
        }
        _ if no_clobber_unsupported(&error.error) => anyhow::anyhow!(
            "failed to atomically persist output firmware '{}': {}; the output file system does \
             not support a no-clobber rename (for example exFAT). Write to a local disk first, \
             then copy the file to a FAT32 USB stick and check its SHA-256 there",
            output_path.display(),
            error.error
        ),
        _ => anyhow::anyhow!(
            "failed to atomically persist output firmware '{}': {}",
            output_path.display(),
            error.error
        ),
    })?;
    if let Err(error) = sync_output_directory(output_dir) {
        let removed = fs::remove_file(output_path).is_ok();
        return Err(error.context(format!(
            "output firmware '{}' may not be durable ({}); do not use it",
            output_path.display(),
            if removed {
                "removed"
            } else {
                "could not be removed"
            }
        )));
    }
    Ok(sha256)
}

/// Whether a persist failed because the file system lacks a no-clobber rename. Rust does not map
/// every platform's "not supported" code to [`ErrorKind::Unsupported`] (macOS reports exFAT as
/// `ENOTSUP`, 45), so the raw codes are checked too.
fn no_clobber_unsupported(error: &std::io::Error) -> bool {
    #[cfg(unix)]
    let raw = {
        let code = error.raw_os_error();
        code == Some(libc::ENOTSUP) || code == Some(libc::EOPNOTSUPP)
    };
    #[cfg(not(unix))]
    let raw = false;
    raw || error.kind() == ErrorKind::Unsupported
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

/// Reads `path` back through the file system and requires exactly `expected`; returns the
/// SHA-256 of the bytes read.
///
/// This catches write-path errors and a wrong file at `path`. It does not prove what is on the
/// storage medium, because the read may be served from the operating system's cache: check a USB
/// stick's file again after ejecting and re-inserting it.
///
/// # Errors
///
/// If `path` cannot be read as a regular file, or its bytes differ from `expected`.
pub fn read_back(path: &Path, expected: &[u8]) -> Result<String> {
    let written = read_regular_file(path)
        .with_context(|| format!("failed to read back '{}'", path.display()))?;
    if written != expected {
        bail!(
            "'{}' does not match the verified bytes after writing; do not use it",
            path.display()
        );
    }
    Ok(sha256_hex(&written))
}
