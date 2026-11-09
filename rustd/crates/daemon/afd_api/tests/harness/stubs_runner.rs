//! The runner plane's stub: a lease plane that by default answers no-work.

use af_core::id::Uuid7;
use afd_api::services::Leasing;
use jiff::Timestamp;

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
    refusal: Option<fn() -> afd_fleet::Error>,
    minted: Option<(&'static str, Timestamp)>,
}

impl NoWork {
    /// A plane whose hydrate, capture, recall and mint answer `refusal`.
    pub(crate) fn refusing(refusal: fn() -> afd_fleet::Error) -> Self {
        Self {
            refusal: Some(refusal),
            ..Self::default()
        }
    }

    /// A plane whose mint hands back `token`, expiring at `expires_at`,
    /// for the one case that proves how the route renders a success.
    pub(crate) fn minting(token: &'static str, expires_at: Timestamp) -> Self {
        Self {
            minted: Some((token, expires_at)),
            ..Self::default()
        }
    }

    /// The chosen refusal, when there is one; `Ok` lets the verb answer as
    /// a plane with no work would.
    fn refused(&self) -> afd_fleet::Result<()> {
        self.refusal.map_or(Ok(()), |refusal| Err(refusal()))
    }
}

impl Leasing for NoWork {
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
        _request: &af_wire::credentials::MintCredentialRequest<'_>,
        _now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<afd_credential::credential::Minted>> + Send {
        if let Some((token, expires_at)) = self.minted {
            return std::future::ready(Ok(afd_credential::credential::Minted {
                token: token.to_owned().into(),
                expires_at,
                rotated_refresh_token: None,
            }));
        }
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
    ) -> impl Future<Output = afd_fleet::Result<af_wire::memory::MemoryHydrateResponse<'static>>> + Send
    {
        std::future::ready(
            self.refused()
                .map(|()| af_wire::memory::MemoryHydrateResponse {
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
        _request: &af_wire::memory::MemoryPushRequest<'_>,
        _now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<afd_memory::Captured>> + Send {
        std::future::ready(self.refused().map(|()| afd_memory::Captured::default()))
    }

    /// Finds nothing, which is what a fleet that has never run remembers.
    fn recall(
        &self,
        _runner_id: &Uuid7,
        _fleet_id: &Uuid7,
        _request: &af_wire::memory::MemoryRecallRequest<'_>,
    ) -> impl Future<Output = afd_fleet::Result<af_wire::memory::MemoryRecallResponse<'static>>> + Send
    {
        std::future::ready(
            self.refused()
                .map(|()| af_wire::memory::MemoryRecallResponse {
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
        request: &af_wire::tool_detail::ToolCallRecordsRequest<'_>,
        _now: Timestamp,
    ) -> impl Future<Output = afd_fleet::Result<af_wire::tool_detail::ToolCallRecordsStored>> + Send
    {
        std::future::ready(Ok(af_wire::tool_detail::ToolCallRecordsStored {
            stored_count: request.calls.len(),
            skipped_count: 0,
        }))
    }

    fn context_load(
        &self,
        _runner: &Uuid7,
        _lease: &Uuid7,
        _branch: &Uuid7,
        _fence: u64,
    ) -> impl Future<Output = afd_fleet::Result<Option<af_context::Checkpoint>>> + Send {
        std::future::ready(Err(afd_fleet::Error::lease_not_found()))
    }

    fn context_commit(
        &self,
        _runner: &Uuid7,
        _lease: &Uuid7,
        _branch: &Uuid7,
        _fence: u64,
        _expected: u64,
        _checkpoint: af_context::Checkpoint,
    ) -> impl Future<Output = afd_fleet::Result<()>> + Send {
        std::future::ready(Err(afd_fleet::Error::lease_not_found()))
    }

    /// Holds no lease, so every lease-addressed verb is refused as one this
    /// runner does not hold — the answer a plane with no rows gives.
    fn standing(
        &self,
        _runner_id: &Uuid7,
        _lease_id: Uuid7,
        _fencing_token: u64,
    ) -> impl Future<Output = afd_fleet::Result<afd_fleet::lease::Standing>> + Send {
        std::future::ready(Err(afd_fleet::Error::lease_not_found()))
    }

    /// Refused for the reason [`NoWork::standing`] is.
    fn message(
        &self,
        _runner_id: &Uuid7,
        _lease_id: Uuid7,
        _request: &af_wire::message_verb::MessageRequest<'_>,
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

impl afd_api::services::RunnerStreams for NoWork {
    async fn open(
        &self,
        _runner: afd_auth::principal::Runner,
        _request: axum::extract::Request,
    ) -> axum::response::Response {
        use axum::response::IntoResponse as _;
        afd_http::envelope::ProblemResponse::new(
            af_core::error_code::INTERNAL_OPERATION_FAILED,
            "This router fixture has no native runner connection.",
            afd_http::request_id::RequestId::mint(),
        )
        .into_response()
    }
}
