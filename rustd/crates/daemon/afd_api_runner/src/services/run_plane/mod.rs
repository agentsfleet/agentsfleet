//! The runner plane's HTTP seam: what the run family of verbs acts through.

use af_core::id::Uuid7;
use af_wire::access_verb::AccessRequest;
use af_wire::credentials::MintCredentialRequest;
use af_wire::memory::{
    MemoryLoadResponse, MemoryPushRequest, MemoryRecallRequest, MemoryRecallResponse,
};
use af_wire::message_verb::MessageRequest;
use af_wire::tool_detail::{ToolCallRecordsRequest, ToolCallRecordsStored};
use af_wire::trigger_verb::TriggerAddRequest;
use afd_credential::credential::Minted;
use afd_fleet::run::Standing;
use afd_fleet::run::access::AccessAsked;
use afd_fleet::run::effect::{Claim, Effect};
use afd_fleet::run::message::SteerSender;
use afd_fleet::run::write_fence::WriteFence;
use afd_memory::Captured;
use afd_outbound::Interim;
use afd_setup::trigger::TriggerRow;
use jiff::Timestamp;

/// The fenced HTTP operations that remain outside the host session.
pub trait RunPlane: Send + Sync + std::fmt::Debug + 'static {
    /// The memory window that seeds one run.
    ///
    /// # Errors
    /// Refuses a runner holding no live run on the fleet, and reports a
    /// datastore that would not answer.
    fn load(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
    ) -> impl Future<Output = afd_fleet::Result<MemoryLoadResponse<'static>>> + Send;

    /// Persist what one run learned.
    ///
    /// # Errors
    /// Refuses a run that is not this runner's or not this fleet's, and a
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
    /// Refuses as [`RunPlane::capture`] does, fenced the same way.
    fn recall(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        request: &MemoryRecallRequest<'_>,
    ) -> impl Future<Output = afd_fleet::Result<MemoryRecallResponse<'static>>> + Send;

    /// Mint one short-lived credential for a running child.
    ///
    /// Every refusal is an ERROR carrying its own registry code: a child is
    /// blocked on this answer and each refusal has a different remedy —
    /// reconnect the integration, wait for a human, re-raise a card, retry
    /// shortly.
    ///
    /// # Errors
    /// Refuses a run that is not this runner's or is no longer live, a fleet
    /// with no approved grant for the integration, a write mint with no usable
    /// approval, an integration this workspace has not connected, and every way
    /// an exchange can fail to produce a credential.
    fn mint(
        &self,
        runner_id: &Uuid7,
        request: &MintCredentialRequest<'_>,
        now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<Minted>> + Send;

    /// Keep each finished call's full arguments and output under the run's
    /// fence.
    ///
    /// # Errors
    /// Refuses a run that is not this runner's or not live, and a holder the
    /// fleet has superseded. A record refused for its shape, a bound or the
    /// event's budget is counted in the answer, not an error.
    fn record_tool_calls(
        &self,
        runner_id: &Uuid7,
        run_id: &str,
        request: &ToolCallRecordsRequest<'_>,
        now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<ToolCallRecordsStored>> + Send;

    /// Proves `run_id` is this runner's live run and the fleet's current
    /// holder, and answers the fleet, workspace and event it runs.
    ///
    /// What the schedules verb acts through before it touches a schedule: the
    /// fleet is the run's, never the request's.
    ///
    /// # Errors
    /// Refuses a run that is not this runner's or not live, and a holder the
    /// fleet has superseded.
    fn standing(
        &self,
        runner_id: &Uuid7,
        run_id: Uuid7,
        fencing_token: u64,
    ) -> impl Future<Output = afd_fleet::Result<Standing>> + Send;

    /// One line for the run's thread: fenced, counted against the run's
    /// cap, and scrubbed of the fleet's secret values, ready to post.
    ///
    /// # Errors
    /// Refuses as [`RunPlane::standing`] does, an event that recorded no
    /// thread, and a run that already said its fill.
    fn message(
        &self,
        runner_id: &Uuid7,
        run_id: Uuid7,
        request: &MessageRequest<'_>,
    ) -> impl Future<Output = afd_fleet::Result<Interim>> + Send;

    /// Claims the item a message, schedule or cron call names: the answer a
    /// first request stored, or the hold under which this one runs the effect.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    fn claim_effect(
        &self,
        standing: &Standing,
        effect: Effect<'_>,
    ) -> impl Future<Output = afd_fleet::Result<Claim>> + Send;

    /// `text` with the run's fleet's declared static secrets masked: what a
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

    /// Who a fleet's steer is from: the run's fleet's saved name, and
    /// `text` masked as [`RunPlane::masked`] masks it, from one read.
    ///
    /// # Errors
    /// Refuses a fleet no longer installed; reports a vault that would not
    /// answer and a mask that could not be built.
    fn sender(
        &self,
        standing: &Standing,
        text: &str,
    ) -> impl Future<Output = afd_fleet::Result<SteerSender>> + Send;

    /// Asks a person for `request`'s name on behalf of the run, or answers
    /// that the fleet's approved grant already covers it.
    ///
    /// # Errors
    /// Refuses as [`RunPlane::standing`] does, a name the workspace lacks, an
    /// integration it has not connected, and a scope or host the request
    /// cannot carry; every refusal writes no grant and raises no card.
    fn request_access(
        &self,
        runner_id: &Uuid7,
        run_id: Uuid7,
        request: &garde::Valid<AccessRequest<'_>>,
        now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<AccessAsked>> + Send;

    /// Adds the trigger `request` names to the run's fleet, as the row
    /// `item_id`, under the run's write fence.
    ///
    /// # Errors
    /// A provider the workspace has not connected, a signing secret its vault
    /// lacks, a shape the trigger parser refuses, a kind and source the fleet
    /// already holds, and a run superseded before the write.
    fn add_trigger(
        &self,
        standing: &Standing,
        item_id: &Uuid7,
        request: &TriggerAddRequest<'_>,
        guard: &WriteFence,
    ) -> impl Future<Output = afd_fleet::Result<TriggerRow>> + Send;

    /// The run's fleet's triggers, oldest first.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    fn triggers(
        &self,
        standing: &Standing,
    ) -> impl Future<Output = afd_fleet::Result<Vec<TriggerRow>>> + Send;

    /// Removes one of the run's fleet's triggers; `false` when it holds no
    /// such trigger.
    ///
    /// # Errors
    /// A run superseded before the write, and datastore failures.
    fn remove_trigger(
        &self,
        standing: &Standing,
        trigger: &Uuid7,
        guard: &WriteFence,
    ) -> impl Future<Output = afd_fleet::Result<bool>> + Send;
}

mod run_service;
