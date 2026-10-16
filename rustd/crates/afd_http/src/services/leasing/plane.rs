//! Production forwarding stays beside the seam it implements.

use super::{
    ActivityFrame, Captured, Interim, Leasing, MemoryHydrateResponse, MemoryPushRequest,
    MemoryRecallRequest, MemoryRecallResponse, MessageRequest, MintCredentialRequest, Minted,
    Reconciled, RenewRequest, ReportRequest, Standing, ToolCallRecordsRequest,
    ToolCallRecordsStored, UnixMillis, Uuid7,
};
use afd_fleet::lease::Plane;

/// The production plane answers it directly.
///
/// The trait is this crate's and the type is `afd_fleet`'s, which is the right
/// direction: the HTTP layer states what it needs, and the service crate stays
/// unaware that an HTTP layer exists.
impl Leasing for Plane {
    fn lease(
        &self,
        drafts: &afd_authoring::Drafts,
        runner_id: &Uuid7,
        held: &[Uuid7],
        accepted: afd_wire::lease::AcceptedKinds,
        degraded: bool,
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<String>> + Send {
        Self::claim(self, drafts, runner_id, held, accepted, degraded, now)
    }

    fn report(
        &self,
        runner_id: &Uuid7,
        request: &ReportRequest<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<Reconciled>> + Send {
        Self::report(self, runner_id, request, now)
    }

    // `async fn`, where the two methods above are `-> impl Future`: this is the
    // one implementation that has work to do AFTER the await — dropping the
    // charge — so it cannot just forward the plane's future. The charge is
    // dropped HERE rather than never produced, so the plane keeps one shape for
    // both metered verbs and M181 §5 has the same seam to attach to on each.
    fn activity(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        frames: &[ActivityFrame<'_>],
    ) -> impl Future<Output = afd_fleet::Result<()>> + Send {
        Self::activity(self, runner_id, lease_id, frames)
    }

    fn hydrate(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<MemoryHydrateResponse<'static>>> + Send {
        Self::hydrate(self, runner_id, fleet_id, now)
    }

    fn recall(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        request: &MemoryRecallRequest<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<MemoryRecallResponse<'static>>> + Send {
        Self::recall(self, runner_id, fleet_id, request, now)
    }

    fn mint(
        &self,
        runner_id: &Uuid7,
        request: &MintCredentialRequest<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<Minted>> + Send {
        Self::mint(self, runner_id, request, now)
    }

    fn capture(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        request: &MemoryPushRequest<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<Captured>> + Send {
        Self::capture(self, runner_id, fleet_id, request, now)
    }

    fn record_tool_calls(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        request: &ToolCallRecordsRequest<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<ToolCallRecordsStored>> + Send {
        Self::record_tool_calls(self, runner_id, lease_id, request, now)
    }

    fn standing(
        &self,
        runner_id: &Uuid7,
        lease_id: Uuid7,
        fencing_token: u64,
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<Standing>> + Send {
        Self::standing(self, runner_id, lease_id, fencing_token, now)
    }

    fn message(
        &self,
        runner_id: &Uuid7,
        lease_id: Uuid7,
        request: &MessageRequest<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<Interim>> + Send {
        Self::message(self, runner_id, lease_id, request, now)
    }

    fn masked(
        &self,
        standing: &Standing,
        text: &str,
    ) -> impl Future<Output = afd_fleet::Result<String>> + Send {
        Self::masked(self, standing, text)
    }

    async fn renew(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        request: RenewRequest,
        now: UnixMillis,
    ) -> afd_fleet::Result<UnixMillis> {
        Self::renew(self, runner_id, lease_id, request, now)
            .await
            .map(|(expires_at, _charged)| expires_at)
    }
}
