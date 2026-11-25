//! `fleet.fleets` and its `fleet.triggers` rows, read the one way an
//! unauthenticated delivery may read them.
//!
//! # Why the statements select the row and stop
//!
//! A trigger row is read back through `af_fleet_runtime`'s trigger parser,
//! the one its add went through (`crate::binding`), so the statements select
//! the columns and stop. Matching an event list or a repository list in SQL
//! would be a second reader of one row nobody could test in isolation.
//!
//! # Every read keeps its workspace predicate
//!
//! A trigger row carries its fleet's workspace, and each statement joins the
//! fleet on both, so a delivery resolved through one workspace's install never
//! reaches another workspace's fleet.

/// The fleet a webhook delivery is addressed to, and its row for the source.
///
/// `$1` fleet, `$2` the webhook trigger type, `$3` the URL's source, lowercase. No
/// filter beyond the identifier and the row, and that is correct rather than
/// lax — the caller is a provider holding no principal, so there is no scope
/// to narrow by. What authorizes the delivery is the signature checked
/// against the secret this row leads to, and never the row's own visibility.
///
/// `status` is read even though no trigger mentions it: a delivery to a fleet
/// that is not active is answered 200 and dropped rather than run.
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

/// Every active fleet in a workspace subscribed to this provider's App events.
///
/// `$1` workspace, `$2` the status a fleet must hold, `$3` the App event trigger type,
/// `$4` provider. No grant join: receiving an event needs no grant, and a
/// grant governs only what the fleet may call.
///
/// The statement asks the RELATIONAL half — which fleets in the workspace
/// subscribed — and the row half, the repository and event lists, is answered
/// by `Binding::serves_repository` and `Binding::admits`, where a test can
/// reach both.
///
/// Ordered by id so every replica fans out in the same order.
pub const SELECT_APP_SUBSCRIBERS: &str = include_str!("select_app_subscribers.sql");

/// The fleets in a workspace subscribed to one chat channel, for
/// [`crate::slack::subscribed`] to read.
///
/// `$1` workspace, `$2` the statuses a subscriber can be read in, `$3` the
/// mention trigger type, `$4` provider, `$5` channel, `$6` the GitHub grant's name,
/// `$7` the approved status. The approved GitHub scope rides along because a
/// fleet that may write to a repository takes only mentions addressed to it.
///
/// Ordered by id so every replica routes the same set the same way.
pub const SELECT_MENTION_CANDIDATES: &str = include_str!("select_mention_candidates.sql");

/// Existing channel notice carrier, scoped to the resolved workspace.
/// `$1` provider, `$2` team, `$3` channel, `$4` workspace.
pub const SELECT_NOTICE_OWNER: &str = include_str!("select_notice_owner.sql");

/// Bind once per channel/workspace; concurrent losers read the committed winner.
///
/// A moved team's prior-workspace binding is repointed, never reused.
///
/// `$1` id, `$2` provider, `$3` team, `$4` channel, `$5` fleet, `$6` workspace.
pub const INSERT_NOTICE_OWNER: &str = include_str!("insert_notice_owner.sql");
