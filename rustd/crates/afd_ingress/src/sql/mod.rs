//! `fleet.fleets` and its head revision, read the one way an unauthenticated
//! delivery may read them.
//!
//! # Why the statements select the document and stop
//!
//! A fleet's document is its head revision's `definition->'config'`.
//! [`afd_fleet_runtime::FleetConfig::stored`] already parses it on the claim
//! path, applies the schema bounds, and completes a trigger's signature block
//! from the provider registry — so the statement here selects the document and
//! stops, instead of walking the trigger array in SQL with
//! `jsonb_array_elements`. That keeps the lookup to one round trip, keeps the
//! "first webhook trigger" rule out of a sub-select nobody can test in
//! isolation, leaves no second statement to disagree about WHICH trigger it
//! read, and refuses a document that fails its bounds here rather than reading
//! half of it.
//!
//! A fleet with no head revision is still installing and has no document. The
//! reads join the head revision, so such a fleet is absent from them: it has
//! no trigger to fire and nothing to subscribe with.

/// Everything an ingress needs to decide about one fleet.
///
/// `$1` fleet. Three columns: the owning workspace, the status, and the
/// document. No filter beyond the identifier, and that is correct rather than
/// lax — the caller is a provider holding no principal, so there is no scope to
/// narrow by. What authorizes the delivery is the signature checked against the
/// secret this row leads to, and never the row's own visibility.
///
/// `status` is read even though nothing in the trigger set mentions it: a
/// delivery to a paused fleet is answered 200 and dropped rather than run, and
/// a query that did not return the status would have no way to know.
pub const SELECT_FLEET_INGRESS: &str = include_str!("select_fleet_ingress.sql");

/// The workspace an App installation was connected to.
///
/// `$1` provider, `$2` the provider's own account identifier — GitHub's
/// `installation.id`, Slack's `team_id`. `connector.installs` carries
/// `UNIQUE (provider, external_account_id)`, so this reads at most one row and
/// needs no `LIMIT`.
///
/// This is the whole reason the table exists: a signed App delivery arrives
/// addressed only by the PROVIDER's identifier and carries no workspace, no
/// fleet and no principal. Without this index there is nothing to route it by.
pub const SELECT_INSTALL_WORKSPACE: &str = include_str!("select_install_workspace.sql");

/// Every fleet in a workspace that could take this provider's App delivery.
///
/// `$1` workspace, `$2` the status a fleet must hold, `$3` the granted service,
/// `$4` the grant status. Four columns per row, which is exactly
/// [`Binding::read_for_source`](crate::Binding::read_for_source)'s input.
///
/// # What this asks, and what it deliberately does not
///
/// Matching the repository and the event in SQL would mean walking the
/// document with `jsonb_array_elements` inside two nested `EXISTS` clauses —
/// the document reader written in SQL that [`SELECT_FLEET_INGRESS`] also
/// avoids. This statement asks only the RELATIONAL half — which fleets are in
/// the workspace, running, and hold an approved grant — and the document half
/// is answered by `Binding::serves_repository` and `Binding::admits`, where a
/// test can reach both.
///
/// The candidate rows crossing the wire are not pre-filtered by repository, so
/// a workspace pays for its active granted fleets rather than its subscribed
/// ones. The set is bounded by the same ceiling the fan-out is — a workspace
/// with more matching fleets than [`crate::MAX_FANOUT`] has a delivery this
/// daemon refuses either way.
///
/// Ordered by id so a truncated fan-out is the same set every replica sees.
pub const SELECT_APP_SUBSCRIBERS: &str = include_str!("select_app_subscribers.sql");

/// The fleets in a workspace a chat mention could reach, for
/// [`crate::slack::subscribed`] to read.
///
/// The relational half only, as [`SELECT_APP_SUBSCRIBERS`] is: whether a
/// fleet's `mention` trigger names the channel is a question about its
/// document, answered where a test can reach it. No grant join — a fleet
/// answering a mention mints no chat credential; the daemon posts for it.
///
/// Ordered by id so every replica routes the same set the same way.
///
/// `$1` workspace, `$2` the statuses a subscriber can be read in.
pub const SELECT_MENTION_CANDIDATES: &str = include_str!("select_mention_candidates.sql");

/// Existing channel notice carrier, scoped to the resolved workspace regardless of binding kind.
/// `$1` provider, `$2` team, `$3` channel, `$4` workspace.
pub const SELECT_NOTICE_OWNER: &str = include_str!("select_notice_owner.sql");

/// Bind once per channel/workspace; concurrent losers read the committed winner.
///
/// A moved team's prior-workspace binding is repointed, never reused.
///
/// `$1` id, `$2` provider, `$3` team, `$4` channel, `$5` fleet, `$6` kind, `$7` workspace.
pub const INSERT_NOTICE_OWNER: &str = include_str!("insert_notice_owner.sql");
