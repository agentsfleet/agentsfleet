//! Production forwarding stays beside the seam it implements.

use super::{
    Captured, Interim, MemoryHydrateResponse, MemoryPushRequest, MemoryRecallRequest,
    MemoryRecallResponse, MessageRequest, MintCredentialRequest, Minted, RunPlane, Standing,
    SteerSender, Timestamp, ToolCallRecordsRequest, ToolCallRecordsStored, Uuid7,
};
use afd_fleet::run::RunService;

/// The production plane answers it directly.
///
/// The trait is this crate's and the type is `afd_fleet`'s, which is the right
/// direction: the HTTP layer states what it needs, and the service crate stays
/// unaware that an HTTP layer exists.
impl RunPlane for RunService {
    fn hydrate(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
    ) -> impl Future<Output = afd_fleet::Result<MemoryHydrateResponse<'static>>> + Send {
        Self::hydrate(self, runner_id, fleet_id)
    }

    fn recall(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        request: &MemoryRecallRequest<'_>,
    ) -> impl Future<Output = afd_fleet::Result<MemoryRecallResponse<'static>>> + Send {
        Self::recall(self, runner_id, fleet_id, request)
    }

    fn mint(
        &self,
        runner_id: &Uuid7,
        request: &MintCredentialRequest<'_>,
        now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<Minted>> + Send {
        Self::mint(self, runner_id, request, now)
    }

    fn capture(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        request: &MemoryPushRequest<'_>,
        now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<Captured>> + Send {
        Self::capture(self, runner_id, fleet_id, request, now)
    }

    fn record_tool_calls(
        &self,
        runner_id: &Uuid7,
        run_id: &str,
        request: &ToolCallRecordsRequest<'_>,
        now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<ToolCallRecordsStored>> + Send {
        Self::record_tool_calls(self, runner_id, run_id, request, now)
    }

    fn standing(
        &self,
        runner_id: &Uuid7,
        run_id: Uuid7,
        fencing_token: u64,
    ) -> impl Future<Output = afd_fleet::Result<Standing>> + Send {
        Self::standing(self, runner_id, run_id, fencing_token)
    }

    fn message(
        &self,
        runner_id: &Uuid7,
        run_id: Uuid7,
        request: &MessageRequest<'_>,
    ) -> impl Future<Output = afd_fleet::Result<Interim>> + Send {
        Self::message(self, runner_id, run_id, request)
    }

    fn masked(
        &self,
        standing: &Standing,
        text: &str,
    ) -> impl Future<Output = afd_fleet::Result<String>> + Send {
        Self::masked(self, standing, text)
    }

    fn sender(
        &self,
        standing: &Standing,
        text: &str,
    ) -> impl Future<Output = afd_fleet::Result<SteerSender>> + Send {
        Self::sender(self, standing, text)
    }
}
