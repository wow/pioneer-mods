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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn identifies_file_size_and_sha256() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let input_path = tempdir.path().join("XDJ700.UPD");
        let mut f = File::create(&input_path).expect("create temp file");
        f.write_all(b"abc").expect("write temp file");
        f.flush().expect("flush temp file");

        let identity = identify_firmware(&input_path).expect("identify firmware");

        assert_eq!(identity.file_name, "XDJ700.UPD");
        assert_eq!(identity.size_bytes, 3);
        assert_eq!(
            identity.sha256_hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
