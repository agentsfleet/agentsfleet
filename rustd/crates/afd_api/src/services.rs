//! What a HANDLER reaches for, as distinct from what `/readyz` consults.
//!
//! Two traits over one state value, split where the seam actually is.
//! [`crate::router::Dependencies`] answers "can this instance take traffic",
//! which is a question about connections; this one answers "what does this verb
//! act through", which is a question about services. A probe that grew a
//! `runners()` method would be asking a readiness check to know about the
//! runner plane.
//!
//! # Why the state is a trait and not a struct
//!
//! The authenticator's concrete type carries three parameters — a directory, a
//! capability source, and a token verifier — and every one of them is chosen by
//! the binary, not by this crate. A concrete state struct would put all three
//! on `build`, on every handler signature, and on every test fixture. One
//! associated type collapses them, and the request path still costs no virtual
//! call because the trait is taken as a generic parameter (`M-DI-HIERARCHY`).
//!
//! # Why the clock is here
//!
//! `afd_core::clock` asks callers to take an instant as a PARAMETER wherever
//! the decision can be handed one, and reserves injection for a long-lived
//! owner that reads repeatedly. The router is that owner: it lives for the
//! process, and every verb under it needs the instant its writes are stamped
//! with. Reading the wall clock inside each handler instead would put a
//! non-deterministic call in the one place a test most needs to pin.

use afd_admin::{Models, PlatformKeys};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet::Runners;
use afd_fleet::bundle::Bundles;
use afd_fleet::credential::Minted;
use afd_fleet::lease::Plane;
use afd_fleet::memory::Captured;
use afd_fleet::money::Nanos;
use afd_fleet::streams::LiveStreams;
use afd_fleet_ops::RunnerLeaseHistory;
use afd_library::{Libraries, LibraryImports};
use afd_wire::activity::ActivityFrame;
use afd_wire::credentials::MintCredentialRequest;
use afd_wire::memory::{MemoryDelta, MemoryPushRequest};
use afd_wire::report::{RenewRequest, ReportRequest};

use crate::auth::Authenticator;

/// The services one request is served through.
///
/// Implemented by the binary's composition root. A suite implements it too —
/// against an in-memory directory and a pool that answers nothing — which is
/// what puts the whole refusal matrix in a test with no datastore in it.
pub trait Services: Send + Sync + std::fmt::Debug + 'static {
    /// What proves a credential on either plane.
    type Auth: Authenticator;

    /// The authenticator every guarded route is proven against.
    fn authenticator(&self) -> &Self::Auth;

    /// The runner control plane's store.
    fn runners(&self) -> &Runners;

    /// What the lease verb acts through.
    ///
    /// An associated type for the reason [`Services::Auth`] is one: the
    /// concrete plane holds a Redis connection that is opened by CONNECTING,
    /// so a suite proving the router's refusal matrix cannot construct one and
    /// must not need to. The binary supplies `afd_fleet::lease::Plane`; a test
    /// supplies whatever answers.
    type Leases: Leasing;

    /// The lease verb's plane: claims, gates, money, credentials.
    ///
    /// Separate from [`Services::runners`] because they are different
    /// questions over different tables — the runner store answers "what is
    /// this host", and this answers "what may it run next". A single accessor
    /// returning both would put the money path behind every heartbeat.
    fn leases(&self) -> &Self::Leases;

    /// The Fleet Bundle snapshot store.
    ///
    /// A concrete type where [`Services::Leases`] is an associated one, and the
    /// difference is what each of them is over. A lease plane holds a Redis
    /// connection opened by CONNECTING, so a suite cannot build one; a bundle
    /// store holds an `Arc<dyn ObjectStore>`, and `object_store` already ships
    /// the in-memory backend a suite drives it with. The seam is inside the
    /// type, so it does not also need to be a parameter on this trait.
    ///
    /// Not an `Option`. A deployment with no snapshot storage answers
    /// `Bundles::unconfigured`, which refuses with a registry code and a
    /// sentence like every other failure on this plane — see
    /// [`afd_fleet::bundle::Bundles`] for why the absence is a value rather
    /// than a `None` each handler would have to render for itself.
    fn bundles(&self) -> &Bundles;

    /// Instance-local live stream metadata for the operator overview.
    fn streams(&self) -> &LiveStreams;

    /// Read-only cross-table projections for fleet operators.
    fn runner_lease_history(&self) -> &RunnerLeaseHistory;

    /// Priced-model catalogue administration.
    fn models(&self) -> &Models;

    /// Reveal-free platform-default administration.
    fn platform_keys(&self) -> &PlatformKeys;

    /// Platform Fleet-library catalogue administration.
    fn libraries(&self) -> &Libraries;

    /// Validated Fleet-library source and snapshot onboarding.
    fn library_imports(&self) -> &LibraryImports;

    /// The instant this request's writes are stamped with.
    ///
    /// Read ONCE per verb and threaded through it, so every row one request
    /// writes carries the same instant — the property `heartbeat.zig` loses by
    /// calling `clock.nowMillis()` separately in each of its four writes, which
    /// leaves a beat's liveness stamp a millisecond or two after its own
    /// transition event.
    fn now(&self) -> UnixMillis;
}

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
