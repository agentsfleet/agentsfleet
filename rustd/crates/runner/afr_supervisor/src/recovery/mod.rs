//! Trusted frozen-workspace recovery over the existing native checkpoint resources.

mod activity;
mod admission;
mod capture;
mod catalog;
#[cfg(test)]
mod catalog_tests;
mod files;
#[cfg(test)]
mod files_tests;
mod inventory;
mod layout;
mod links;
mod names;
mod progress;
#[cfg(test)]
mod progress_tests;
mod restore;
mod restore_manifest;
mod session;
#[cfg(test)]
mod supervisor_tests;
#[cfg(test)]
mod tests;
mod transfer;
#[cfg(test)]
mod transfer_tests;
mod turn;
mod uploads;
mod window;

pub(crate) use admission::RecoveryResources;
pub(super) use capture::Capture;
pub(super) use restore::Restore;
pub(crate) use transfer::Transfers;
pub(super) use turn::NativeTurn;

#[cfg(test)]
pub(crate) use transfer::http_tests::server::Server as TestHttpsServer;

use crate::client::ControlPlane;
use af_core::id::Uuid7;
use af_wire::durable_task::{CheckpointPlan, DecisionDurableTaskVersion};

/// One native pin owner. It never extends the original capture/restore deadline.
#[derive(Debug)]
pub(super) struct Session<'a> {
    plane: &'a ControlPlane,
    run: &'a Uuid7,
    workspace: &'a Uuid7,
    fence: u64,
    task: DecisionDurableTaskVersion,
    plan: CheckpointPlan,
    activity_id: String,
    progress: &'a mut progress::Progress,
    transfers: Transfers,
    stop: &'a tokio_util::sync::CancellationToken,
}

/// The workspace's checkpoint key rides each object; it is wiped on every early
/// return as well as after successful use.
#[derive(Debug)]
pub(super) struct Object(af_wire::durable_task::CheckpointObject);

impl From<af_wire::durable_task::CheckpointObject> for Object {
    fn from(object: af_wire::durable_task::CheckpointObject) -> Self {
        Self(object)
    }
}
impl std::ops::Deref for Object {
    type Target = af_wire::durable_task::CheckpointObject;
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
