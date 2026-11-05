//! Supervisor-only checkpoint protocol; keys and signed URLs never enter chat.

mod manifest;
mod objects;
mod plan;

pub use manifest::{CheckpointEntry, CheckpointFile, CheckpointPage, CheckpointPageInput};
pub use objects::{
    CheckpointAuthorizationInput, CheckpointAuthorizations, CheckpointObject,
    CheckpointObjectStatus, CheckpointObjectsInput, CheckpointObjectsPage, CheckpointPlaintext,
    CheckpointTransfer, CheckpointUpload, CheckpointUploadsInput,
};
pub use plan::{
    CheckpointCommitInput, CheckpointMode, CheckpointPin, CheckpointPinInput, CheckpointPlan,
    CheckpointPlanInput, CheckpointReadQuery, CheckpointTransferPurpose,
};

/// Current independently validated capture/restore format.
pub const CHECKPOINT_FORMAT: u32 = 1;
/// Largest plaintext chunk retained in one authenticated buffer.
pub const CHECKPOINT_OBJECT_BYTES: u64 = 8 * 1024 * 1024;
/// Maximum bytes in a complete serialized page.
pub const CHECKPOINT_PAGE_BYTES: usize = 1024 * 1024;
/// Maximum pages in the complete logical manifest.
pub const CHECKPOINT_PAGES: u32 = 64;
/// Maximum objects registered or authorized in one request.
pub const CHECKPOINT_BATCH: usize = 256;

pub(super) fn digest(value: &str, _context: &()) -> garde::Result {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(garde::Error::new("expected lowercase SHA-256"))
    }
}
