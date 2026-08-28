//! The preference bag and the onboarding checklist, as the dashboard reads them.
//!
//! # A preference value rides back exactly as it arrived
//!
//! `prefs` maps each key to a `&RawValue`, not to a parsed `serde_json::Value`.
//! The column stores the client's own bytes, and re-emitting them verbatim is
//! what makes a preference round-trip byte for byte — a re-serialized `Value`
//! would normalise number formatting and key order on a payload the server has
//! no business normalising.
//!
//! # The bag is an object, and an empty one is the unset answer
//!
//! `{"prefs":{}}` is what a person who has set nothing gets. Never a 404, and
//! never an absent field: the dashboard fails open toward SHOWING onboarding,
//! so the shape it reads on a miss has to be the shape it reads on a hit.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::value::RawValue;

/// `GET /v1/workspaces/{workspace_id}/preferences` and the response every
/// preference write answers with.
///
/// A write returns the WHOLE bag rather than the one key it set, matching
/// `respondWithBag`: the dashboard holds the bag in one piece of state, so
/// handing back a fragment would make it merge on the client instead.
#[derive(Debug, Clone, Serialize)]
pub struct PreferencesResponse<'a> {
    /// Every key this person has set in this workspace, key-ordered.
    ///
    /// A `BTreeMap` so the order is the SQL's `ORDER BY pref_key` and stays
    /// stable between two captures — object key order is not load-bearing for a
    /// client, and is exactly what makes a diff of two responses readable.
    pub prefs: BTreeMap<&'a str, &'a RawValue>,
}

/// `GET /v1/workspaces/{workspace_id}/onboarding` — the whole checklist.
///
/// Five derived signals and three preference reads in one call, which is the
/// consolidation this endpoint exists for: the dashboard used to fetch it as
/// six separate requests.
///
/// Eight booleans, and `struct_excessive_bools` is right that this usually
/// means a missing type. Not here: this IS the wire shape, field for field, and
/// the dashboard destructures all eight. Collapsing any of them into an enum
/// would change the JSON a shipped client reads.
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
    /// The person ticked the install-the-CLI step by hand.
    pub cli_ticked: bool,
    /// The person dismissed the panel outright.
    pub dismissed: bool,
    /// The panel is collapsed but not dismissed.
    pub collapsed: bool,
}
