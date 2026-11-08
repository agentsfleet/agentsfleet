//! Trusted frozen-workspace recovery over the existing native checkpoint resources.

mod activity;
mod capture;
mod catalog;
#[cfg(test)]
mod catalog_tests;
mod files;
#[cfg(test)]
mod files_tests;
mod links;
mod progress;
#[cfg(test)]
mod progress_tests;
mod restore;
mod session;
#[cfg(test)]
mod tests;
mod transfer;
#[cfg(test)]
mod transfer_tests;

pub(super) use capture::Capture;
pub(super) use restore::Restore;

use crate::client::ControlPlane;
use af_core::id::Uuid7;
use af_wire::task::{CheckpointPlan, DecisionTaskVersion};

/// Four maximum-size ciphertexts, or one native page of smaller objects.
const UPLOAD_BATCH_BYTES: u64 = 4 * (files::CHUNK + af_crypto::TAG_LEN as u64);

/// One native pin owner. It never extends the original capture/restore deadline.
#[derive(Debug)]
pub(super) struct Session<'a> {
    plane: &'a ControlPlane,
    lease: &'a Uuid7,
    workspace: &'a Uuid7,
    fence: u64,
    task: DecisionTaskVersion,
    plan: CheckpointPlan,
    activity_id: String,
    progress: &'a mut progress::Progress,
    transfers: transfer::Transfers,
    stop: &'a tokio_util::sync::CancellationToken,
}

/// Native temporary keys are wiped on every early return as well as successful use.
#[derive(Debug)]
pub(super) struct Object(af_wire::task::CheckpointObject);

impl From<af_wire::task::CheckpointObject> for Object {
    fn from(object: af_wire::task::CheckpointObject) -> Self {
        Self(object)
    }
}
impl std::ops::Deref for Object {
    type Target = af_wire::task::CheckpointObject;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for Object {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for Object {
    fn drop(&mut self) {
        use zeroize::Zeroize as _;
        if let Some(key) = self.0.data_key.as_mut() {
            key.zeroize();
        }
    }
}
