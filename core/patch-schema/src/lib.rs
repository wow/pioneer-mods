use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeManifest {
    pub schema_version: u32,
    pub recipe_id: String,
    pub description: String,
    pub targets: Vec<SupportedFirmware>,
    pub operations: Vec<PatchOperation>,
}

impl RecipeManifest {
    pub fn validate(&self) -> Result<(), SchemaValidationError> {
        if self.schema_version == 0 {
            return Err(SchemaValidationError::SchemaVersionZero);
        }
        if self.recipe_id.trim().is_empty() {
            return Err(SchemaValidationError::EmptyRecipeId);
        }
        if self.targets.is_empty() {
            return Err(SchemaValidationError::NoTargets);
        }
        if self.operations.is_empty() {
            return Err(SchemaValidationError::NoOperations);
        }

        for op in &self.operations {
            match op {
                PatchOperation::WriteSpan(span) => {
                    if span.length == 0 {
                        return Err(SchemaValidationError::ZeroLengthWriteSpan);
                    }
                    if span.bytes.len() != span.length as usize {
                        return Err(SchemaValidationError::WriteSpanLengthMismatch);
                    }
                }
                PatchOperation::OwnerCopyWindow(window) => {
                    if window.length == 0 {
                        return Err(SchemaValidationError::ZeroLengthOwnerCopyWindow);
                    }
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportedFirmware {
    pub model: String,
    pub version: String,
    pub size_bytes: u64,
    pub sha256_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PatchOperation {
    WriteSpan(WriteSpan),
    OwnerCopyWindow(OwnerCopyWindow),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteSpan {
    pub offset: u64,
    pub length: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerCopyWindow {
    pub source_offset: u64,
    pub destination_offset: u64,
    pub length: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SchemaValidationError {
    #[error("schema_version must be >= 1")]
    SchemaVersionZero,
    #[error("recipe_id must not be empty")]
    EmptyRecipeId,
    #[error("targets list must not be empty")]
    NoTargets,
    #[error("operations list must not be empty")]
    NoOperations,
    #[error("write span length must be > 0")]
    ZeroLengthWriteSpan,
    #[error("write span bytes length does not match declared length")]
    WriteSpanLengthMismatch,
    #[error("owner copy window length must be > 0")]
    ZeroLengthOwnerCopyWindow,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> RecipeManifest {
        RecipeManifest {
            schema_version: 1,
            recipe_id: "xdj700-waveform3".to_owned(),
            description: "Enable 3-band waveform rendering".to_owned(),
            targets: vec![SupportedFirmware {
                model: "XDJ-700".to_owned(),
                version: "1.15".to_owned(),
                size_bytes: 16,
                sha256_hex: "00".repeat(32),
            }],
            operations: vec![PatchOperation::WriteSpan(WriteSpan {
                offset: 4,
                length: 3,
                bytes: vec![1, 2, 3],
            })],
        }
    }

    #[test]
    fn validates_well_formed_manifest() {
        assert!(valid_manifest().validate().is_ok());
    }

    #[test]
    fn rejects_invalid_write_span_length_mismatch() {
        let mut manifest = valid_manifest();
        let PatchOperation::WriteSpan(span) = &mut manifest.operations[0] else {
            panic!("unexpected operation");
        };
        span.length = 10;
        assert_eq!(
            manifest.validate(),
            Err(SchemaValidationError::WriteSpanLengthMismatch)
        );
    }
}
