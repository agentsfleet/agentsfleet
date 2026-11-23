//! The one round trip the onboarding checklist is derived from, and the
//! platform-default read beside it.

/// Every derivable onboarding signal, in one round trip.
///
/// Five `EXISTS` subqueries rather than five requests: the planner stops at the
/// first matching row, so none of these scans a table. The workspace signals
/// key on the `workspace_id` index and the tenant-model check on the
/// one-selected exclusion index of `model.tenant_models`.
///
/// `$2` is the steer-actor prefix, bound rather than inlined (RULE NSQ).
pub const SELECT_SIGNALS: &str = include_str!("onboarding/select_signals.sql");

/// Whether an active platform default points at a model.
///
/// The second half of `model_configured`: a fresh tenant with no selection of
/// its own rides the platform default, so the checklist must not tell them to
/// configure a model they already have. Read as its own statement because the
/// row lives in a different table than the five signals above.
pub const SELECT_PLATFORM_DEFAULT_MODEL: &str =
    include_str!("onboarding/select_platform_default_model.sql");
