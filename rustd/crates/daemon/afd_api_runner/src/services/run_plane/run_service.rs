//! Production forwarding stays beside the seam it implements.

use super::{
    Captured, Interim, MemoryLoadResponse, MemoryPushRequest, MemoryRecallRequest,
    MemoryRecallResponse, MessageRequest, MintCredentialRequest, Minted, RunPlane, Standing,
    SteerSender, Timestamp, ToolCallRecordsRequest, ToolCallRecordsStored, Uuid7,
};
use af_wire::access_verb::AccessRequest;
use af_wire::trigger_verb::TriggerAddRequest;
use afd_fleet::run::RunService;
use afd_fleet::run::access::AccessAsked;
use afd_fleet::run::effect::{Claim, Effect};
use afd_fleet::run::write_fence::WriteFence;
use afd_setup::trigger::TriggerRow;

/// The production plane answers it directly.
///
/// The trait is this crate's and the type is `afd_fleet`'s, which is the right
/// direction: the HTTP layer states what it needs, and the service crate stays
/// unaware that an HTTP layer exists.
impl RunPlane for RunService {
    fn web_search(
        &self,
        runner: &Uuid7,
        run: Uuid7,
        request: &garde::Valid<af_wire::web::SearchRequest>,
        deadline: tokio::time::Instant,
    ) -> impl Future<Output = afd_fleet::Result<af_wire::web::SearchResponse>> + Send {
        Self::web_search(self, runner, run, request, deadline)
    }

    fn web_extract(
        &self,
        runner: &Uuid7,
        run: Uuid7,
        request: &garde::Valid<af_wire::web::ExtractRequest>,
        deadline: tokio::time::Instant,
    ) -> impl Future<Output = afd_fleet::Result<af_wire::web::ExtractResponse>> + Send {
        Self::web_extract(self, runner, run, request, deadline)
    }

    fn load(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
    ) -> impl Future<Output = afd_fleet::Result<MemoryLoadResponse<'static>>> + Send {
        Self::load(self, runner_id, fleet_id)
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

    fn claim_effect(
        &self,
        standing: &Standing,
        effect: Effect<'_>,
    ) -> impl Future<Output = afd_fleet::Result<Claim>> + Send {
        Self::claim_effect(self, standing, effect)
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

    fn request_access(
        &self,
        runner_id: &Uuid7,
        run_id: Uuid7,
        request: &garde::Valid<AccessRequest<'_>>,
        now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<AccessAsked>> + Send {
        Self::request_access(self, runner_id, run_id, request, now)
    }

    fn add_trigger(
        &self,
        standing: &Standing,
        item_id: &Uuid7,
        request: &TriggerAddRequest<'_>,
        guard: &WriteFence,
    ) -> impl Future<Output = afd_fleet::Result<TriggerRow>> + Send {
        Self::add_trigger(self, standing, item_id, request, guard)
    }

    fn triggers(
        &self,
        standing: &Standing,
    ) -> impl Future<Output = afd_fleet::Result<Vec<TriggerRow>>> + Send {
        Self::triggers(self, standing)
    }

    fn remove_trigger(
        &self,
        standing: &Standing,
        trigger: &Uuid7,
        guard: &WriteFence,
    ) -> impl Future<Output = afd_fleet::Result<bool>> + Send {
        Self::remove_trigger(self, standing, trigger, guard)
    }
}
