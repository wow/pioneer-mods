use crate::error::PatchCoreError;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FirmwareIdentity {
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256_hex: String,
}

/// Streams `path` and returns its identity without keeping the bytes in memory.
pub fn identify_firmware(path: &Path) -> Result<FirmwareIdentity, PatchCoreError> {
    let mut file = open_regular_file(path)?;

    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size_bytes = 0_u64;

    loop {
        let read_bytes = file
            .read(&mut buffer)
            .map_err(|source| read_error(path, source))?;
        if read_bytes == 0 {
            break;
        }
        hasher.update(&buffer[..read_bytes]);
        size_bytes += read_bytes as u64;
    }

    Ok(FirmwareIdentity {
        file_name: firmware_file_name(path),
        size_bytes,
        sha256_hex: hex::encode(hasher.finalize()),
    })
}

/// Reads `path` once and returns its identity together with the exact bytes that were hashed.
///
/// Refuses anything that is not a regular file; see [`read_regular_file`].
pub fn read_firmware(path: &Path) -> Result<(FirmwareIdentity, Vec<u8>), PatchCoreError> {
    let bytes = read_regular_file(path)?;
    let identity = identify_bytes(firmware_file_name(path), &bytes);
    Ok((identity, bytes))
}

/// Reads a regular file completely, without hashing it.
///
/// Directories, FIFOs, sockets and devices are refused, so callers never block on a pipe or read
/// an unbounded device.
pub fn read_regular_file(path: &Path) -> Result<Vec<u8>, PatchCoreError> {
    let mut file = open_regular_file(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|source| read_error(path, source))?;
    Ok(bytes)
}

pub fn identify_bytes(file_name: String, input_bytes: &[u8]) -> FirmwareIdentity {
    FirmwareIdentity {
        file_name,
        size_bytes: input_bytes.len() as u64,
        sha256_hex: sha256_hex(input_bytes),
    }
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Display name for a firmware path: the UTF-8 file name, or the full (lossy) path otherwise.
pub fn firmware_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Opens `path` only if it is a regular file.
///
/// The path is checked first so that devices are normally never opened. On Unix the file is then
/// opened with `O_NONBLOCK`, so a path swapped to a FIFO after the check cannot block `open()`.
/// The open handle is checked again before any read; `O_NONBLOCK` has no effect on reads from a
/// regular file.
fn open_regular_file(path: &Path) -> Result<File, PatchCoreError> {
    let not_a_file = || PatchCoreError::InputNotAFile {
        path: path.to_string_lossy().into_owned(),
    };
    let metadata = path.metadata().map_err(|source| read_error(path, source))?;
    if !metadata.is_file() {
        return Err(not_a_file());
    }
    let file = open_options()
        .open(path)
        .map_err(|source| read_error(path, source))?;
    let handle_metadata = file.metadata().map_err(|source| read_error(path, source))?;
    if !handle_metadata.is_file() {
        return Err(not_a_file());
    }
    Ok(file)
}

#[cfg(unix)]
fn open_options() -> OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = OpenOptions::new();
    options.read(true).custom_flags(libc::O_NONBLOCK);
    options
}

#[cfg(not(unix))]
fn open_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true);
    options
}

fn read_error(path: &Path, source: std::io::Error) -> PatchCoreError {
    PatchCoreError::ReadFile {
        path: path.to_string_lossy().into_owned(),
        source,
    }
}
