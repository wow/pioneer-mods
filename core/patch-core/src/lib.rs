pub mod engine;
pub mod error;
pub mod identity;
pub mod upd;

pub use engine::{PatchOutcome, apply_recipe, verify_bounded_diff};
pub use error::{
    LayoutViolation, OperationRegion, PatchCoreError, PatchEngineError, RecordDefect, UpdError,
};
pub use identity::{
    FirmwareIdentity, firmware_file_name, identify_bytes, identify_firmware, read_firmware,
    read_regular_file, sha256_hex,
};
pub use upd::{UpdContainer, parse_upd};
