use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeManifest {
    pub schema_version: u32,
    pub recipe_id: String,
    pub description: String,
    pub targets: Vec<SupportedFirmware>,
    pub operations: Vec<PatchOperation>,
}

impl RecipeManifest {
    pub fn validate(&self) -> Result<(), SchemaValidationError> {
        if self.schema_version != 1 {
            return Err(SchemaValidationError::UnsupportedSchemaVersion {
                schema_version: self.schema_version,
            });
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

        for target in &self.targets {
            if target.model.trim().is_empty() {
                return Err(SchemaValidationError::EmptyTargetModel);
            }
            if target.version.trim().is_empty() {
                return Err(SchemaValidationError::EmptyTargetVersion);
            }
            if !is_valid_sha256_hex(&target.sha256_hex) {
                return Err(SchemaValidationError::InvalidTargetSha256Hex);
            }
            if let Some(expected_output_sha256) = &target.expected_output_sha256
                && !is_valid_sha256_hex(expected_output_sha256)
            {
                return Err(SchemaValidationError::InvalidExpectedOutputSha256Hex);
            }
        }

        for op in &self.operations {
            match op {
                PatchOperation::WriteSpan(span) => {
                    if span.length == 0 {
                        return Err(SchemaValidationError::ZeroLengthWriteSpan);
                    }
                    if span.bytes.len() as u64 != span.length {
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

    pub fn matching_target(&self, size_bytes: u64, sha256_hex: &str) -> Option<&SupportedFirmware> {
        self.targets
            .iter()
            .find(|target| target.matches_identity(size_bytes, sha256_hex))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportedFirmware {
    pub model: String,
    pub version: String,
    pub size_bytes: u64,
    pub sha256_hex: String,
    #[serde(default)]
    pub expected_output_sha256: Option<String>,
}

impl SupportedFirmware {
    pub fn matches_identity(&self, size_bytes: u64, sha256_hex: &str) -> bool {
        self.size_bytes == size_bytes && self.sha256_hex.eq_ignore_ascii_case(sha256_hex)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PatchOperation {
    WriteSpan(WriteSpan),
    OwnerCopyWindow(OwnerCopyWindow),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteSpan {
    pub offset: u64,
    pub length: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerCopyWindow {
    /// Copy semantics always read from original owner input bytes, not from partially patched output bytes.
    pub source_offset: u64,
    pub destination_offset: u64,
    pub length: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SchemaValidationError {
    #[error(
        "unsupported schema_version: {schema_version}; only schema_version=1 is currently supported"
    )]
    UnsupportedSchemaVersion { schema_version: u32 },
    #[error("recipe_id must not be empty")]
    EmptyRecipeId,
    #[error("targets list must not be empty")]
    NoTargets,
    #[error("operations list must not be empty")]
    NoOperations,
    #[error("target model must not be empty")]
    EmptyTargetModel,
    #[error("target version must not be empty")]
    EmptyTargetVersion,
    #[error("target sha256_hex must be 64 hexadecimal characters")]
    InvalidTargetSha256Hex,
    #[error("target expected_output_sha256 must be 64 hexadecimal characters when provided")]
    InvalidExpectedOutputSha256Hex,
    #[error("write span length must be > 0")]
    ZeroLengthWriteSpan,
    #[error("write span bytes length does not match declared length")]
    WriteSpanLengthMismatch,
    #[error("owner copy window length must be > 0")]
    ZeroLengthOwnerCopyWindow,
}

fn is_valid_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.as_bytes().iter().all(u8::is_ascii_hexdigit)
}
