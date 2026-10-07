//! Output-file safety shared by the commands that write firmware: never the input path, no
//! overwrite unless forced, and an atomic, synced write whose bytes are read back before they
//! appear under the output name.

use anyhow::{Context, Result, bail};
use patch_core::open_regular_file;
use std::fs;
use std::io::{ErrorKind, Read, Write};
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

/// Refuses, before any work is done, an output path that resolves to the input, whose directory
/// does not exist, or that already exists (including as a dangling symlink) unless overwriting
/// is forced.
///
/// # Errors
///
/// On aliasing, a missing output directory, an existing output, or a path that cannot be
/// resolved.
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

    let output_dir = output_dir(output_path);
    if !output_dir.is_dir() {
        bail!(
            "output directory '{}' does not exist or is not a directory",
            output_dir.display()
        );
    }
    // `symlink_metadata` also sees a dangling symlink, which `exists()` would miss.
    if fs::symlink_metadata(output_path).is_ok() && overwrite != Overwrite::Forced {
        return Err(overwrite.refusal(output_path));
    }

    Ok(())
}

fn output_dir(output_path: &Path) -> &Path {
    match output_path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    }
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

/// Writes `bytes` to `output_path` atomically.
///
/// The bytes go to a temporary file in the same directory (never under the output name), which
/// is synced and then read back through the file system ([`read_back`]) before it is renamed into
/// place, so only bytes that read back exactly can appear under `output_path`. The temporary file
/// is deleted on any error. After the rename the directory is synced. If that fails, the file is
/// kept, because its bytes are verified and removing it would not make anything durable (with
/// `--force` the previous file is already replaced), and the error says durability is
/// unconfirmed. A crash after the rename can only leave the complete, verified file, because the
/// rename is atomic.
///
/// # Errors
///
/// An overwrite refusal (see [`Overwrite`]), a write, sync or read-back failure, a file system
/// without a no-clobber rename ([`is_no_clobber_unsupported`]), or a failed directory sync.
pub fn write_output_atomically(
    output_path: &Path,
    bytes: &[u8],
    overwrite: Overwrite,
) -> Result<()> {
    let output_dir = output_dir(output_path);
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
    read_back(temp_file.path(), bytes).with_context(|| {
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
        _ if is_no_clobber_unsupported(&error.error) => anyhow::anyhow!(
            "failed to atomically persist output firmware '{}': {}; the output file system may \
             not support a no-clobber rename (for example FAT or exFAT on some systems). Write \
             the output to a local disk instead",
            output_path.display(),
            error.error
        ),
        _ => anyhow::anyhow!(
            "failed to atomically persist output firmware '{}': {}",
            output_path.display(),
            error.error
        ),
    })?;
    sync_output_directory(output_dir).with_context(|| {
        format!(
            "output firmware '{}' was written and verified, but its directory could not be \
             synced, so durability is not confirmed; check the file again before use",
            output_path.display()
        )
    })?;
    Ok(())
}

/// Whether a persist error probably means the file system lacks a no-clobber rename.
///
/// Rust does not map every platform's "not supported" code to [`ErrorKind::Unsupported`]: macOS
/// reports exFAT as `ENOTSUP` (45). On Linux, when the rename flag is unsupported the temporary
/// file falls back to a hard link, which FAT and exFAT refuse with `EPERM`; so `EPERM` is
/// included, and the message says "may".
pub fn is_no_clobber_unsupported(error: &std::io::Error) -> bool {
    #[cfg(unix)]
    let raw = {
        let code = error.raw_os_error();
        [libc::ENOTSUP, libc::EOPNOTSUPP, libc::EPERM]
            .iter()
            .any(|&known| code == Some(known))
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

/// Reads `path` back through the file system in chunks and requires exactly `expected`.
///
/// This catches write-path errors and a wrong file at `path`. It does not prove what is on the
/// storage medium, because the read may be served from the operating system's cache: check a USB
/// stick's file again after ejecting and re-inserting it.
///
/// # Errors
///
/// If `path` cannot be read as a regular file, or its bytes differ from `expected`.
pub fn read_back(path: &Path, expected: &[u8]) -> Result<()> {
    let mismatch = || {
        anyhow::anyhow!(
            "'{}' does not match the verified bytes after writing; do not use it",
            path.display()
        )
    };
    let failed = |error: std::io::Error| {
        anyhow::Error::new(error).context(format!("failed to read back '{}'", path.display()))
    };
    let mut file = open_regular_file(path)
        .with_context(|| format!("failed to read back '{}'", path.display()))?;
    let len = file.metadata().map_err(failed)?.len();
    if len != expected.len() as u64 {
        return Err(mismatch());
    }
    let mut buffer = vec![0u8; READ_BACK_CHUNK];
    let mut offset = 0;
    loop {
        let read = file.read(&mut buffer).map_err(failed)?;
        if read == 0 {
            break;
        }
        if expected.get(offset..offset + read) != Some(&buffer[..read]) {
            return Err(mismatch());
        }
        offset += read;
    }
    if offset != expected.len() {
        return Err(mismatch());
    }
    Ok(())
}

const READ_BACK_CHUNK: usize = 1 << 20;
