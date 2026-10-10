//! The runner plane's stub: a lease plane that by default answers no-work.

use std::sync::{Arc, Mutex};

use afd_api::services::Leasing;
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet::lease::report::Reconciled;

/// A lease plane that by default answers no-work.
///
/// The production plane holds a Dragonfly connection that is opened by CONNECTING,
/// so these suites cannot build one — and should not: what they prove is the
/// router's guard, scope and refusal matrix, which is decided BEFORE any verb
/// runs. A stub that answers each verb the same way keeps that boundary honest,
/// because a suite here cannot accidentally start asserting on lease
/// behaviour that belongs to `afd_fleet`'s own integration lane.
///
/// It remembers the holds each poll named, which is the one thing the lease
/// handler decides before the plane: what the poll's body reads as.
///
/// [`NoWork::refusing`] makes the memory verbs and the mint answer one chosen
/// refusal instead, for a suite that proves how a route RENDERS a refusal the
/// plane decided: the status and `current_state` are the handler's, not the
/// stub's.
#[derive(Debug, Clone, Default)]
pub(crate) struct NoWork {
    polled: Arc<Mutex<Vec<Vec<String>>>>,
    refusal: Option<fn() -> afd_fleet::Error>,
}

impl NoWork {
    /// A plane whose hydrate, capture, recall and mint answer `refusal`.
    pub(crate) fn refusing(refusal: fn() -> afd_fleet::Error) -> Self {
        Self {
            refusal: Some(refusal),
            ..Self::default()
        }
    }

    /// The fleets each poll named as held, oldest poll first.
    pub(crate) fn polled_holds(&self) -> Vec<Vec<String>> {
        self.polled.lock().expect("the poll log is healthy").clone()
    }

    /// The chosen refusal, when there is one; `Ok` lets the verb answer as
    /// a plane with no work would.
    fn refused(&self) -> afd_fleet::Result<()> {
        self.refusal.map_or(Ok(()), |refusal| Err(refusal()))
    }
}

impl Leasing for NoWork {
    fn lease(
        &self,
        _runner_id: &Uuid7,
        held: &[Uuid7],
        _degraded: bool,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<String>> + Send {
        let named = held.iter().map(|fleet| fleet.as_str().to_owned()).collect();
        self.polled
            .lock()
            .expect("the poll log is healthy")
            .push(named);
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
            tenant_id: fixture_id(),
            event_id: String::new(),
            posture: String::new(),
            provider: String::new(),
            model: String::new(),
            // A FIRST report, not a repeat: the handler skips its analytics,
            // meters and delivery span on a repeat, and a stub answering
            // `true` would route every suite here down the quiet arm and
            // prove the loud one is reachable never.
            repeated: false,
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
    /// platform credential answers, unless built with [`NoWork::refusing`].
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
        let refusal = self.refusal.unwrap_or(afd_fleet::Error::mint_unconfigured);
        std::future::ready(Err(refusal()))
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
    ) -> impl Future<Output = afd_fleet::Result<afd_wire::memory::MemoryHydrateResponse<'static>>> + Send
    {
        std::future::ready(
            self.refused()
                .map(|()| afd_wire::memory::MemoryHydrateResponse {
                    memory: Vec::new(),
                    shared: Vec::new(),
                    publish: false,
                }),
        )
    }

    /// Stores nothing and says so, for the reason [`NoWork::report`] accepts.
    fn capture(
        &self,
        _runner_id: &Uuid7,
        _fleet_id: &Uuid7,
        _request: &afd_wire::memory::MemoryPushRequest<'_>,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<afd_memory::Captured>> + Send {
        std::future::ready(self.refused().map(|()| afd_memory::Captured::default()))
    }

    /// Finds nothing, which is what a fleet that has never run remembers.
    fn recall(
        &self,
        _runner_id: &Uuid7,
        _fleet_id: &Uuid7,
        _request: &afd_wire::memory::MemoryRecallRequest<'_>,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<afd_wire::memory::MemoryRecallResponse<'static>>> + Send
    {
        std::future::ready(
            self.refused()
                .map(|()| afd_wire::memory::MemoryRecallResponse {
                    memory: Vec::new(),
                    shared: Vec::new(),
                }),
        )
    }

    /// Keeps every record a post carries, which is what a plane with room
    /// for all of them would do.
    fn record_tool_calls(
        &self,
        _runner_id: &Uuid7,
        _lease_id: &str,
        request: &afd_wire::tool_detail::ToolCallRecordsRequest<'_>,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<afd_wire::tool_detail::ToolCallRecordsStored>> + Send
    {
        std::future::ready(Ok(afd_wire::tool_detail::ToolCallRecordsStored {
            stored_count: request.calls.len(),
            skipped_count: 0,
        }))
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

    /// Holds no lease, so every lease-addressed verb is refused as one this
    /// runner does not hold — the answer a plane with no rows gives.
    fn standing(
        &self,
        _runner_id: &Uuid7,
        _lease_id: Uuid7,
        _fencing_token: u64,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<afd_fleet::lease::Standing>> + Send {
        std::future::ready(Err(afd_fleet::Error::lease_not_found()))
    }

    /// Refused for the reason [`NoWork::standing`] is.
    fn message(
        &self,
        _runner_id: &Uuid7,
        _lease_id: Uuid7,
        _request: &afd_wire::message_verb::MessageRequest<'_>,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<afd_outbound::Interim>> + Send {
        std::future::ready(Err(afd_fleet::Error::lease_not_found()))
    }

    /// Refused for the reason [`NoWork::standing`] is.
    fn masked(
        &self,
        _standing: &afd_fleet::lease::Standing,
        _text: &str,
    ) -> impl Future<Output = afd_fleet::Result<String>> + Send {
        std::future::ready(Err(afd_fleet::Error::lease_not_found()))
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
