//! The runner plane's seam: what the lease family of verbs acts through.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet::credential::Minted;
use afd_fleet::lease::Plane;
use afd_fleet::memory::Captured;
use afd_fleet::money::Nanos;
use afd_wire::activity::ActivityFrame;
use afd_wire::credentials::MintCredentialRequest;
use afd_wire::memory::{MemoryDelta, MemoryPushRequest};
use afd_wire::report::{RenewRequest, ReportRequest};

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
    /// not deliver.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, or a stored configuration
    /// this daemon cannot read. Every DECISION — no work, refused, parked — is
    /// an `Ok` carrying the bytes that say so.
    fn lease(
        &self,
        runner_id: &Uuid7,
        degraded: bool,
        now: UnixMillis,
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
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<Nanos>> + Send;

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
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<UnixMillis>> + Send;

    /// The memory window that seeds one run.
    ///
    /// # Errors
    /// Refuses a runner holding no live lease on the fleet, and reports a
    /// datastore that would not answer.
    fn hydrate(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<Vec<MemoryDelta<'static>>>> + Send;

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
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<Captured>> + Send;

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
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<Minted>> + Send;

    /// Forward one batch of live-tail frames.
    ///
    /// No clock parameter, and it is the only verb here without one: nothing
    /// this publishes is stamped or stored, so there is no row for an instant
    /// to be consistent with. A `now` threaded in anyway would be an argument
    /// that exists to look like its neighbours.
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
}

/// The production plane answers it directly.
///
/// The trait is this crate's and the type is `afd_fleet`'s, which is the right
/// direction: the HTTP layer states what it needs, and the service crate stays
/// unaware that an HTTP layer exists.
impl Leasing for Plane {
    fn lease(
        &self,
        runner_id: &Uuid7,
        degraded: bool,
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<String>> + Send {
        Self::lease(self, runner_id, degraded, now)
    }

    fn report(
        &self,
        runner_id: &Uuid7,
        request: &ReportRequest<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<Nanos>> + Send {
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
    ) -> impl Future<Output = afd_fleet::Result<Vec<MemoryDelta<'static>>>> + Send {
        Self::hydrate(self, runner_id, fleet_id, now)
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
