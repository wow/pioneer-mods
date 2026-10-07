use patch_schema::SchemaValidationError;
use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PatchCoreError {
    #[error("input path does not point to a regular file: {path}")]
    InputNotAFile { path: String },

    #[error("failed to read firmware file '{path}': {source}")]
    ReadFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Refusal reasons from [`crate::apply_recipe`]. Every variant means no output may be written.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PatchEngineError {
    #[error("recipe manifest is invalid: {0}")]
    InvalidManifest(#[from] SchemaValidationError),

    #[error(
        "input firmware (size={size_bytes}, sha256={sha256_hex}) is not compatible with recipe '{recipe_id}'"
    )]
    IncompatibleInput {
        recipe_id: String,
        size_bytes: u64,
        sha256_hex: String,
    },

    #[error(
        "operations[{operation_index}] {region}: out of bounds (offset={offset}, length={length}, input_size={input_size})"
    )]
    RangeOutOfBounds {
        operation_index: usize,
        region: OperationRegion,
        offset: u64,
        length: u64,
        input_size: usize,
    },

    #[error(
        "operations[{operation_index}] destination range {start}..{end} overlaps with operations[{other_operation_index}] range {other_start}..{other_end}"
    )]
    OverlappingDestinations {
        operation_index: usize,
        start: usize,
        end: usize,
        other_operation_index: usize,
        other_start: usize,
        other_end: usize,
    },

    #[error(
        "mutation verification failed: input and output lengths differ (input={input_len}, output={output_len})"
    )]
    OutputLengthChanged { input_len: usize, output_len: usize },

    #[error(
        "mutation verification failed: byte offset {byte_offset} changed outside declared destination ranges"
    )]
    MutationOutsideDeclaredRegions { byte_offset: usize },

    #[error(
        "output SHA-256 mismatch for recipe '{recipe_id}': expected={expected}, actual={actual}"
    )]
    OutputHashMismatch {
        recipe_id: String,
        expected: String,
        actual: String,
    },
}

/// Which range of an operation failed a bounds check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationRegion {
    WriteSpanDestination,
    OwnerCopySource,
    OwnerCopyDestination,
}

impl fmt::Display for OperationRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::WriteSpanDestination => "write_span destination",
            Self::OwnerCopySource => "owner_copy_window source",
            Self::OwnerCopyDestination => "owner_copy_window destination",
        };
        f.write_str(label)
    }
}
