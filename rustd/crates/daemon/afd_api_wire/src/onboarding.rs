//! The onboarding checklist, as the dashboard reads it.

use serde::Serialize;

/// The whole onboarding checklist, in one call.
//
// Five derived signals in one call, which is the consolidation this endpoint
// exists for: the dashboard used to fetch it as separate requests.
//
// Five booleans, and `struct_excessive_bools` is right that this usually
// means a missing type. Not here: this IS the wire shape, field for field, and
// a client destructures all five. Collapsing any of them into an enum would
// change the JSON a shipped client reads.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[expect(
    clippy::struct_excessive_bools,
    reason = "this struct IS the documented response body; see above"
)]
#[derive(Debug, Clone, Copy, Serialize)]
pub struct OnboardingResponse {
    /// A model resolves — the tenant's own selection, or the platform default.
    pub model_configured: bool,
    /// The workspace holds at least one fleet.
    pub has_fleet: bool,
    /// The workspace holds at least one vault secret.
    pub has_secret: bool,
    /// At least one event has reached the workspace.
    pub has_processed_event: bool,
    /// At least one of those events came from a steer.
    pub has_steer_event: bool,
}
