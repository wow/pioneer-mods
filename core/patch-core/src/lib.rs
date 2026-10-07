pub mod engine;
pub mod error;
pub mod identity;

pub use engine::{PatchOutcome, apply_recipe, verify_bounded_diff};
pub use error::{OperationRegion, PatchCoreError, PatchEngineError};
pub use identity::{FirmwareIdentity, identify_bytes, identify_firmware, sha256_hex};
