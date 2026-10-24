//! The runner plane's HTTP seam: what the lease family of verbs acts through.

use afd_core::id::Uuid7;
use afd_credential::credential::Minted;
use afd_fleet::lease::Standing;
use afd_fleet::lease::report::Reconciled;
use afd_memory::Captured;
use afd_outbound::Interim;
use afd_wire::activity::ActivityFrame;
use afd_wire::credentials::MintCredentialRequest;
use afd_wire::memory::{
    MemoryHydrateResponse, MemoryPushRequest, MemoryRecallRequest, MemoryRecallResponse,
};
use afd_wire::message_verb::MessageRequest;
use afd_wire::report::{RenewRequest, ReportRequest};
use afd_wire::tool_detail::{ToolCallRecordsRequest, ToolCallRecordsStored};
use jiff::Timestamp;

/// Answering one runner's poll.
///
/// One method, because the lease verb is one decision — the HTTP layer asks it
/// and renders whatever comes back. The answer is already-serialized JSON:
/// `ExecutionPolicy` borrows from values that do not outlive the call that
/// assembles it, and `afd_fleet::lease` documents why owning or re-assembling
/// are both worse.
pub trait Leasing: Send + Sync + std::fmt::Debug + 'static {
    /// The next lease for `runner_id`, or a backoff.
    ///
    /// `degraded` fails CLOSED: a runner whose verdict could not be read is
    /// issued nothing, because its assignment names an isolation the host may
    /// not deliver. `held` names the fleets whose sandboxes the runner holds,
    /// for the pass to try before its partition.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, or a stored configuration
    /// this daemon cannot read. Every DECISION — no work, refused, parked — is
    /// an `Ok` carrying the bytes that say so.
    fn lease(
        &self,
        drafts: &afd_authoring::Drafts,
        runner_id: &Uuid7,
        held: &[Uuid7],
        accepted: afd_wire::lease::AcceptedKinds,
        degraded: bool,
        now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<String>> + Send;

    /// Record one terminal execution result.
    ///
    /// Answers what the final slice drained. Unlike [`Leasing::lease`], the
    /// decisions here are ERRORS rather than `Ok` values: a stale fence and a
    /// foreign lease are things the runner is TOLD, with a code it acts on,
    /// where "no work" is an ordinary poll outcome that needs no code at all.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's, and a holder the fleet has
    /// superseded. Also reports a datastore that would not answer.
    fn report(
        &self,
        runner_id: &Uuid7,
        request: &ReportRequest<'_>,
        now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<Reconciled>> + Send;

    /// Extend one live lease's deadline, metering the slice since the last.
    ///
    /// Answers the new deadline only. What the slice drained stays inside the
    /// plane: the wire reply carries a deadline and nothing else, and widening
    /// this signature to return a charge nobody renders would be an amount
    /// travelling to a caller that has no use for it.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's, one no longer active, one
    /// past the hard runtime ceiling, and one whose tenant or fleet has run out
    /// of money — each with its own registry code.
    fn renew(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        request: RenewRequest,
        now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<Timestamp>> + Send;

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
    ) -> impl Future<Output = afd_fleet::Result<Option<afd_context::Checkpoint>>> + Send;

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
        checkpoint: afd_context::Checkpoint,
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

    /// Forward one batch of live-tail frames.
    ///
    /// No clock parameter: nothing this publishes is stamped or stored, so
    /// there is no row for an instant to be consistent with.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's, and reports a datastore that
    /// would not answer. A dropped frame is neither.
    fn activity(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        frames: &[ActivityFrame<'_>],
    ) -> impl Future<Output = afd_fleet::Result<()>> + Send;

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

mod plane;
