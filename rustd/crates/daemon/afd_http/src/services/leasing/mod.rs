//! The runner plane's HTTP seam: what the lease family of verbs acts through.

use af_core::id::Uuid7;
use af_wire::credentials::MintCredentialRequest;
use af_wire::memory::{
    MemoryHydrateResponse, MemoryPushRequest, MemoryRecallRequest, MemoryRecallResponse,
};
use af_wire::message_verb::MessageRequest;
use af_wire::tool_detail::{ToolCallRecordsRequest, ToolCallRecordsStored};
use afd_credential::credential::Minted;
use afd_fleet::lease::Standing;
use afd_memory::Captured;
use afd_outbound::Interim;
use jiff::Timestamp;

/// The fenced HTTP operations that remain outside the host session.
pub trait Leasing: Send + Sync + std::fmt::Debug + 'static {
    /// Read private model-window notes under the live assignment.
    ///
    /// # Errors
    /// Refuses an absent or superseded assignment, or unavailable storage.
    fn context_load(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        branch: &Uuid7,
        fence: u64,
    ) -> impl Future<Output = afd_fleet::Result<Option<af_context::Checkpoint>>> + Send;

    /// Publish private notes against the expected generation and assignment.
    ///
    /// # Errors
    /// Refuses changed assignment, scope or generation, or unavailable storage.
    fn context_commit(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        branch: &Uuid7,
        fence: u64,
        expected: u64,
        checkpoint: af_context::Checkpoint,
    ) -> impl Future<Output = afd_fleet::Result<()>> + Send;

    /// The memory window that seeds one run.
    ///
    /// # Errors
    /// Refuses a runner holding no live lease on the fleet, and reports a
    /// datastore that would not answer.
    fn hydrate(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
    ) -> impl Future<Output = afd_fleet::Result<MemoryHydrateResponse<'static>>> + Send;

    /// Persist what one run learned.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's or not this fleet's, and a
    /// holder the fleet has superseded.
    fn capture(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        request: &MemoryPushRequest<'_>,
        now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<Captured>> + Send;

    /// Search a fleet's memory past the window a run was seeded with.
    ///
    /// # Errors
    /// Refuses as [`Leasing::capture`] does, fenced the same way.
    fn recall(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        request: &MemoryRecallRequest<'_>,
    ) -> impl Future<Output = afd_fleet::Result<MemoryRecallResponse<'static>>> + Send;

    /// Mint one short-lived credential for a running child.
    ///
    /// Like [`Leasing::renew`] and unlike [`Leasing::lease`], every refusal is
    /// an ERROR carrying its own registry code: a child is blocked on this
    /// answer and each refusal has a different remedy — reconnect the
    /// integration, wait for a human, re-raise a card, retry shortly — where a
    /// polling runner's only move is to wait.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's or is no longer live, a fleet
    /// with no approved grant for the integration, a write mint with no usable
    /// approval, an integration this workspace has not connected, and every way
    /// an exchange can fail to produce a credential.
    fn mint(
        &self,
        runner_id: &Uuid7,
        request: &MintCredentialRequest<'_>,
        now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<Minted>> + Send;

    /// Keep each finished call's full arguments and output under the lease's
    /// fence.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's or not live, and a holder the
    /// fleet has superseded. A record refused for its shape, a bound or the
    /// event's budget is counted in the answer, not an error.
    fn record_tool_calls(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        request: &ToolCallRecordsRequest<'_>,
        now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<ToolCallRecordsStored>> + Send;

    /// Proves `lease_id` is this runner's live lease and the fleet's current
    /// holder, and answers the fleet, workspace and event it runs.
    ///
    /// What the schedules verb acts through before it touches a schedule: the
    /// fleet is the lease's, never the request's.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's or not live, and a holder the
    /// fleet has superseded.
    fn standing(
        &self,
        runner_id: &Uuid7,
        lease_id: Uuid7,
        fencing_token: u64,
    ) -> impl Future<Output = afd_fleet::Result<Standing>> + Send;

    /// One line for the lease's thread: fenced, counted against the run's
    /// cap, and scrubbed of the fleet's secret values, ready to post.
    ///
    /// # Errors
    /// Refuses as [`Leasing::standing`] does, an event that recorded no
    /// thread, and a run that already said its fill.
    fn message(
        &self,
        runner_id: &Uuid7,
        lease_id: Uuid7,
        request: &MessageRequest<'_>,
    ) -> impl Future<Output = afd_fleet::Result<Interim>> + Send;

    /// `text` with the lease's fleet's declared static secrets masked: what a
    /// schedule the fleet writes stores and hands the scheduler.
    ///
    /// # Errors
    /// Refuses a fleet no longer installed; reports a vault that would not
    /// answer and a mask that could not be built.
    fn masked(
        &self,
        standing: &Standing,
        text: &str,
    ) -> impl Future<Output = afd_fleet::Result<String>> + Send;
}

mod lease_service;
