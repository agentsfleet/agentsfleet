//! The draft's compacted context as a setup run reaches it: the daemon's
//! fenced context verb, spoken on the run's own connection under its fence.

use af_core::id::Uuid7;
use af_wire::context::{ContextLoadResponse, ContextSaveRequest, ContextSaveResponse};
use afr_agent::SetupContext;

use crate::client::ControlPlane;

/// One setup run's context verb.
#[derive(Debug)]
pub(super) struct RunContext<'a> {
    plane: &'a ControlPlane,
    run: &'a Uuid7,
    fencing_token: u64,
}

impl<'a> RunContext<'a> {
    /// The verb for `run`, which holds its draft at `fencing_token`.
    pub(super) const fn new(plane: &'a ControlPlane, run: &'a Uuid7, fencing_token: u64) -> Self {
        Self {
            plane,
            run,
            fencing_token,
        }
    }
}

#[async_trait::async_trait]
impl SetupContext for RunContext<'_> {
    async fn load(&self) -> afr_agent::Result<ContextLoadResponse> {
        self.plane
            .context_load(self.run, self.fencing_token)
            .await
            .map_err(afr_agent::Error::setup_context)
    }

    /// The run's own fence rides every save, whatever the loop named.
    async fn save(&self, request: ContextSaveRequest) -> afr_agent::Result<ContextSaveResponse> {
        let request = ContextSaveRequest {
            fencing_token: self.fencing_token,
            ..request
        };
        self.plane
            .context_save(self.run, &request)
            .await
            .map_err(afr_agent::Error::setup_context)
    }
}
