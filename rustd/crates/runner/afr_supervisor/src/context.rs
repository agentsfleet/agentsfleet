//! Run working notes use a separate port from reusable semantic memory.
use crate::client::{ControlPlane, retrying};
use af_context::{Checkpoint, Error, Result};
use af_core::id::Uuid7;
use af_wire::context::ContextPut;

#[derive(Debug)]
pub(crate) struct LeaseContext<'a> {
    plane: &'a ControlPlane,
    lease: &'a Uuid7,
    fence: u64,
}
impl<'a> LeaseContext<'a> {
    pub(crate) const fn new(plane: &'a ControlPlane, lease: &'a Uuid7, fence: u64) -> Self {
        Self {
            plane,
            lease,
            fence,
        }
    }
}

#[async_trait::async_trait]
impl afr_agent::ContextCheckpoints for LeaseContext<'_> {
    async fn commit(&self, branch: &str, expected: u64, checkpoint: Checkpoint) -> Result<()> {
        let request = ContextPut {
            fencing_token: self.fence,
            expected_generation: expected,
            checkpoint: serde_json::to_value(checkpoint)?,
        };
        retrying(|| self.plane.context_commit(self.lease, branch, &request))
            .await
            .map_err(Error::boundary)
    }
}
