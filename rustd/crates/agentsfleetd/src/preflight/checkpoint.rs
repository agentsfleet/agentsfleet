//! Optional platform-owned R2 location; credentials stay in the existing vault.

use afd_core::env::{EnvSource, optional};
use afd_core::id::Uuid7;
use afd_fleet::task::storage::CheckpointStorageConfig;
use super::{Fault, PLATFORM_ADMIN_WORKSPACE_KNOB};

/// JSON location and existing platform vault reference; contains no secret bytes.
pub const CHECKPOINT_STORAGE_KNOB: &str = "R2_CHECKPOINT_STORAGE";

pub(super) fn read<E: EnvSource + ?Sized>(env: &E, faults: &mut Vec<Fault>, admin: Option<&Uuid7>)
    -> Option<garde::Valid<CheckpointStorageConfig>> {
    let raw = optional(env, CHECKPOINT_STORAGE_KNOB)?;
    let parsed = serde_json::from_str::<CheckpointStorageConfig>(&raw).ok()
        .and_then(|config| garde::Unvalidated::new(config).validate().ok());
    if parsed.is_none() {
        faults.push(Fault::Invalid { knob: CHECKPOINT_STORAGE_KNOB,
            why: "requires a private R2 account endpoint, bucket and existing credential_name".to_owned() });
    }
    if admin.is_none() {
        faults.push(Fault::Invalid { knob: PLATFORM_ADMIN_WORKSPACE_KNOB,
            why: "checkpoint storage requires the platform Secrets workspace".to_owned() });
    }
    parsed
}
