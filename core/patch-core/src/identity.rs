use crate::error::PatchCoreError;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FirmwareIdentity {
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256_hex: String,
}

pub fn identify_firmware(path: &Path) -> Result<FirmwareIdentity, PatchCoreError> {
    let path_display = path.to_string_lossy().into_owned();

    let metadata = path.metadata().map_err(|source| PatchCoreError::ReadFile {
        path: path_display.clone(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(PatchCoreError::InputNotAFile { path: path_display });
    }

    let mut file = File::open(path).map_err(|source| PatchCoreError::ReadFile {
        path: path_display.clone(),
        source,
    })?;

    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size_bytes = 0_u64;

    loop {
        let read_bytes = file
            .read(&mut buffer)
            .map_err(|source| PatchCoreError::ReadFile {
                path: path_display.clone(),
                source,
            })?;
        if read_bytes == 0 {
            break;
        }
        hasher.update(&buffer[..read_bytes]);
        size_bytes += read_bytes as u64;
    }

    let sha256_hex = hex::encode(hasher.finalize());
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or(path_display);

    Ok(FirmwareIdentity {
        file_name,
        size_bytes,
        sha256_hex,
    })
}

pub fn identify_bytes(file_name: String, input_bytes: &[u8]) -> FirmwareIdentity {
    let mut hasher = Sha256::new();
    hasher.update(input_bytes);
    let sha256_hex = hex::encode(hasher.finalize());

    FirmwareIdentity {
        file_name,
        size_bytes: input_bytes.len() as u64,
        sha256_hex,
    }
}
