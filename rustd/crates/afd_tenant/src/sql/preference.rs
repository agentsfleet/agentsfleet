//! Per-user, per-workspace dashboard preference reads and writes, plus the one
//! round trip the onboarding checklist is derived from.

/// The internal user a Clerk subject maps to.
///
/// The principal carries an opaque external subject; every preference row keys
/// on `identity.users.id`. Two names for one person, and this is the join.
pub const SELECT_USER_ID_BY_SUBJECT: &str =
    include_str!("preference/select_user_id_by_subject.sql");

/// Every preference this user has set in this workspace, key-ordered.
///
/// Ordered so the bag is stable between reads: the response is a JSON object
/// and object key order is not load-bearing, but a stable order makes a diff
/// between two captures readable.
pub const SELECT_BAG: &str = include_str!("preference/select_bag.sql");

/// Writes one key, last-write-wins.
///
/// Arbitrates on `uq_user_preferences_user_id_workspace_id_name`, which is
/// the constraint the schema declares for exactly this statement. A preference
/// is one scalar toggle, so a lost concurrent write costs one click rather than
/// authored content — which is why this is an upsert and not a transaction.
pub const UPSERT_PREF: &str = include_str!("preference/upsert_pref.sql");

/// Every derivable onboarding signal, in one round trip.
///
/// Five `EXISTS` subqueries rather than five requests: the planner stops at the
/// first matching row, so none of these scans a table. The workspace signals
/// key on the `workspace_id` index and the tenant-model check on
/// `model.tenant_selections`'s primary key.
///
/// `$2` is the steer-actor prefix, bound rather than inlined (RULE NSQ).
pub const SELECT_SIGNALS: &str = include_str!("preference/select_signals.sql");

/// Whether an active platform default points at a model.
///
/// The second half of `model_configured`: a fresh tenant with no selection of
/// its own rides the platform default, so the checklist must not tell them to
/// configure a model they already have. Read as its own statement because the
/// row lives in a different table than the five signals above.
pub const SELECT_PLATFORM_DEFAULT_MODEL: &str =
    include_str!("preference/select_platform_default_model.sql");
