//! The runner plane's stub: a lease plane that always answers no-work.

use afd_api::services::Leasing;
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet::lease::report::Reconciled;

/// A lease plane that always answers no-work.
///
/// The production plane holds a Redis connection that is opened by CONNECTING,
/// so these suites cannot build one — and should not: what they prove is the
/// router's guard, scope and refusal matrix, which is decided BEFORE any verb
/// runs. A stub that always answers the same thing keeps that boundary honest,
/// because a suite here cannot accidentally start asserting on lease
/// behaviour that belongs to `afd_fleet`'s own integration lane.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NoWork;

impl Leasing for NoWork {
    fn lease(
        &self,
        _runner_id: &Uuid7,
        _degraded: bool,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<String>> + Send {
        std::future::ready(Ok(r#"{"lease":null,"retry_after_ms":1000}"#.to_owned()))
    }

    /// Accepts every report and charges nothing, which is what a plane with no
    /// work in it would do.
    ///
    /// Deliberately not a refusal. A suite here proves the guard, scope and
    /// refusal matrix in FRONT of the verb, so what it needs is for an
    /// authenticated runner to REACH the handler — and every refusal this verb
    /// can raise needs a real lease row to be refused against, which is
    /// `afd_fleet`'s integration lane and its live Postgres. Returning an error
    /// here would put a code on the wire that no datastore decided, and a
    /// router suite asserting on it would be asserting on this stub.
    fn report(
        &self,
        _runner_id: &Uuid7,
        _request: &afd_wire::report::ReportRequest<'_>,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<Reconciled>> + Send {
        std::future::ready(Ok(Reconciled {
            charged: afd_billing::Nanos::ZERO,
            fleet_id: fixture_id(),
            workspace_id: fixture_id(),
        }))
    }

    /// Accepts every batch of frames and publishes none, which is what a plane
    /// with no queue behind it does.
    ///
    /// The truest of the three stubs: publishing IS best-effort in production,
    /// so a plane that drops every frame and answers `Ok` is not pretending —
    /// it is one end of the range the real verb already spans.
    fn activity(
        &self,
        _runner_id: &Uuid7,
        _lease_id: &str,
        _frames: &[afd_wire::activity::ActivityFrame<'_>],
    ) -> impl Future<Output = afd_fleet::Result<()>> + Send {
        std::future::ready(Ok(()))
    }

    /// Mints nothing, and says so with the code a deployment holding no
    /// platform credential answers.
    ///
    /// A REFUSAL where the three stubs above answer `Ok`, and the asymmetry is
    /// the verb's: `mint` has no success this suite could assert without a
    /// vault row, a grant and a vendor, so an `Ok` here would have to invent a
    /// token. `UZ-CRED-002` is the honest answer for a plane with no platform
    /// credentials in it — the same one production gives — and it still proves
    /// what these suites are for: that an authenticated runner REACHES the
    /// handler and an unauthenticated one does not.
    fn mint(
        &self,
        _runner_id: &Uuid7,
        _request: &afd_wire::credentials::MintCredentialRequest<'_>,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<afd_credential::credential::Minted>> + Send {
        std::future::ready(Err(afd_fleet::Error::mint_unconfigured()))
    }

    /// Hydrates nothing, which is what a fleet that has never run remembers.
    ///
    /// An empty window is a real answer, not a stand-in: a first run seeds from
    /// exactly this.
    fn hydrate(
        &self,
        _runner_id: &Uuid7,
        _fleet_id: &Uuid7,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<Vec<afd_wire::memory::MemoryDelta<'static>>>> + Send
    {
        std::future::ready(Ok(Vec::new()))
    }

    /// Stores nothing and says so, for the reason [`NoWork::report`] accepts.
    fn capture(
        &self,
        _runner_id: &Uuid7,
        _fleet_id: &Uuid7,
        _request: &afd_wire::memory::MemoryPushRequest<'_>,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<afd_fleet::memory::Captured>> + Send {
        std::future::ready(Ok(afd_fleet::memory::Captured::default()))
    }

    /// Renews to the instant asked about, for the reason [`NoWork::report`]
    /// accepts.
    fn renew(
        &self,
        _runner_id: &Uuid7,
        _lease_id: &str,
        _request: afd_wire::report::RenewRequest,
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<UnixMillis>> + Send {
        std::future::ready(Ok(now))
    }
}

/// The identifier a stubbed settle reports against.
///
/// One value for both the fleet and the workspace: nothing in a router suite
/// reads either — the stub exists so an authenticated runner REACHES the
/// handler — and two spellings would suggest a distinction this stub does not
/// make.
fn fixture_id() -> Uuid7 {
    Uuid7::parse("01924f4e-0000-7000-8000-00000000fee7")
        .expect("a fixture identifier is well formed")
}
