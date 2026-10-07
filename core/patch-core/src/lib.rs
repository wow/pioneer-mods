pub mod error;
pub mod identity;

pub use error::PatchCoreError;
pub use identity::{FirmwareIdentity, identify_firmware};
