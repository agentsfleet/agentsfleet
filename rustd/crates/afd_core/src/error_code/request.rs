//! The codes a REQUEST earns, and the ones this instance earns for itself.
//!
//! Two groups in one file because they share a caller's relationship to them:
//! a malformed identifier, an unusable cursor and a name already taken are all
//! things the caller can correct, and a datastore that would not answer or a
//! boot check that failed are things nobody outside this process can. What
//! joins them is that neither group belongs to a PLANE — every plane raises
//! both.

use super::ErrorCode;

/// Identifier failed the canonical version-7 UUID shape (`id_format.zig`).
pub const UUIDV7_INVALID_ID_SHAPE: ErrorCode = ErrorCode::declare("UZ-UUIDV7-009");

/// Request body was malformed or violated a documented bound.
pub const INVALID_REQUEST: ErrorCode = ErrorCode::declare("UZ-REQ-001");

/// A request or fetched artifact exceeded its declared byte or entry bound.
pub const PAYLOAD_TOO_LARGE: ErrorCode = ErrorCode::declare("UZ-REQ-002");

/// A stored envelope was malformed — wrong component length, or an unsupported version.
pub const VAULT_DATA_INVALID: ErrorCode = ErrorCode::declare("UZ-VAULT-001");

/// A stringified secret body past the bound one row may hold.
pub const VAULT_DATA_TOO_LARGE: ErrorCode = ErrorCode::declare("UZ-VAULT-002");

/// This workspace holds no secret under the requested name.
pub const SECRET_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-VAULT-003");

/// Model registry entries still name the secret a delete asked to remove.
pub const SECRET_REFERENCED_BY_MODEL_ENTRIES: ErrorCode = ErrorCode::declare("UZ-VAULT-004");

/// A secret with the requested name already exists in this workspace.
pub const SECRET_NAME_TAKEN: ErrorCode = ErrorCode::declare("UZ-VAULT-005");

/// The path named a fleet this daemon has no row for.
///
/// `ERR_WEBHOOK_FLEET_NOT_FOUND` (`error_entries.zig:133`). Answered before any
/// secret is resolved, and it is the ONE refusal on this surface that a caller
/// earns without presenting a signature — there is no fleet whose secret could
/// have been asked for.
pub const WEBHOOK_FLEET_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-WH-001");

/// A verified delivery whose body is not the shape its header claims.
///
/// `ERR_WEBHOOK_MALFORMED` (`error_entries.zig:134`). Decided AFTER the
/// signature, never before: a body that fails to parse has still proved it came
/// from the holder of the secret, so this is the sender's own bug rather than an
/// intruder's, and telling them so costs nothing.
pub const WEBHOOK_MALFORMED: ErrorCode = ErrorCode::declare("UZ-WH-002");

/// This fleet has no usable webhook signing secret configured.
///
/// `ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED` (`error_registry.zig:67`). An
/// operator's misconfiguration, answered before any verification is attempted —
/// there is nothing to verify a signature against.
pub const WEBHOOK_CREDENTIAL_NOT_CONFIGURED: ErrorCode = ErrorCode::declare("UZ-WH-020");

/// A signed delivery presented no signature, or one that did not match.
///
/// `ERR_WEBHOOK_SIG_INVALID` (`error_registry.zig:65`). One code for absent,
/// malformed and mismatched: telling a sender WHICH way its proof failed
/// narrows a forger's search, and no honest sender acts differently on the
/// three.
///
/// UNIFIED across this surface (M180). The Zig daemon answers
/// `UZ-APPROVAL-003` for approval deliveries and `UZ-SLK-010` for Slack events;
/// the Rust daemon answers this for all of them.
pub const WEBHOOK_SIGNATURE_INVALID: ErrorCode = ErrorCode::declare("UZ-WH-010");

/// A correctly-signed delivery arrived outside its freshness window.
///
/// `ERR_WEBHOOK_TIMESTAMP_STALE` (`error_registry.zig:66`). Distinct from
/// [`WEBHOOK_SIGNATURE_INVALID`] because a provider ACTS on the difference: a
/// late delivery is one to retry, a bad signature is one never to send again.
/// Unified with `UZ-SLK-011` per M180.
pub const WEBHOOK_TIMESTAMP_STALE: ErrorCode = ErrorCode::declare("UZ-WH-011");

/// An App delivery whose installation resolves to no workspace.
///
/// `ERR_WEBHOOK_INSTALL_NOT_MAPPED` (`error_entries.zig:141`). Never a refusal
/// on the wire: the App ingress answers 200 and names this as the reason it
/// dropped the delivery. The sender is a correctly configured provider signing
/// with the right secret — what is missing is a `core.connector_installs` row,
/// which only an operator reconnecting the App can supply. A 4xx would put a
/// delivery nobody can act on into a three-day retry loop.
pub const WEBHOOK_INSTALL_NOT_MAPPED: ErrorCode = ErrorCode::declare("UZ-WH-021");

/// An App delivery no fleet in the resolved workspace subscribed to.
///
/// `ERR_WEBHOOK_SUBSCRIPTION_NOT_FOUND` (`error_entries.zig:142`). The ordinary
/// case rather than a fault, and 200 for the same reason as
/// [`WEBHOOK_INSTALL_NOT_MAPPED`]: an App receives every event for every
/// repository in an installation, and a workspace subscribes a few fleets to a
/// few of them. Almost every delivery this daemon accepts ends here.
pub const WEBHOOK_SUBSCRIPTION_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-WH-022");

/// A delivery whose body is past the ingress limit.
///
/// `ERR_WEBHOOK_PAYLOAD_TOO_LARGE` (`error_entries.zig:143`). Decided on the
/// byte count BEFORE the body is hashed, let alone parsed: the cap is what
/// bounds the work an unauthenticated request can ask this daemon to do, so
/// spending an HMAC over a body to find out it was too big would defeat it.
pub const WEBHOOK_PAYLOAD_TOO_LARGE: ErrorCode = ErrorCode::declare("UZ-WH-030");

/// No approval gate under that id, in that workspace.
pub const APPROVAL_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-APPROVAL-002");

/// No schedule under that identifier belongs to this fleet.
///
/// `ERR_SCHEDULE_NOT_FOUND` (`error_registry.zig:100`). One answer for a
/// schedule that never existed and one belonging to another fleet, for the
/// reason `WEBHOOK_FLEET_NOT_FOUND` gives about its own pair: telling them
/// apart confirms an identifier across an ownership boundary.
///
/// The number is the ZIG's, not this family's ordinal. `UZ-SCHED-001` is
/// `ERR_SCHEDULE_INVALID` over there and a client already branches on it, so
/// numbering this family from one would have made two daemons answer one code
/// with two meanings — a 404 here and a 422 there.
pub const SCHEDULE_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-SCHED-002");

/// This fleet already holds as many schedules as it may.
///
/// `ERR_SCHEDULE_LIMIT_REACHED` (`error_registry.zig:101`).
pub const SCHEDULE_LIMIT_REACHED: ErrorCode = ErrorCode::declare("UZ-SCHED-003");

/// This fleet already registered a schedule under that upstream key.
///
/// `ERR_SCHEDULE_CONFLICT` (`error_registry.zig:106`), whose entry names the
/// source key exactly.
pub const SCHEDULE_KEY_TAKEN: ErrorCode = ErrorCode::declare("UZ-SCHED-008");

/// Another syncer holds this schedule.
///
/// `ERR_SCHEDULE_UPDATE_BUSY` (`error_registry.zig:104`). Distinct from a
/// not-found: the row EXISTS and the caller may retry in a moment, so the two
/// answers send a caller to different places.
pub const SCHEDULE_SYNCING: ErrorCode = ErrorCode::declare("UZ-SCHED-006");

/// The path named a preference key outside the writable registry.
pub const PREF_KEY_UNKNOWN: ErrorCode = ErrorCode::declare("UZ-PREFS-001");

/// A preference value past the one-kibibyte bound a single toggle may hold.
pub const PREF_VALUE_TOO_LARGE: ErrorCode = ErrorCode::declare("UZ-PREFS-002");

/// A workspace in this tenant already uses the requested name.
pub const WORKSPACE_NAME_EXISTS: ErrorCode = ErrorCode::declare("UZ-WORKSPACE-001");

/// `starting_after` is not a cursor the library endpoint issued.
pub const LIBRARY_CURSOR_MALFORMED: ErrorCode = ErrorCode::declare("UZ-LIBRARY-001");

/// A real library cursor, for a different query — its filters or limit differ.
pub const LIBRARY_CURSOR_MISMATCH: ErrorCode = ErrorCode::declare("UZ-LIBRARY-002");

/// A library page or filter input past its documented bound.
pub const LIBRARY_INPUT_OUT_OF_BOUNDS: ErrorCode = ErrorCode::declare("UZ-LIBRARY-003");

/// The library read's statement failed transiently.
pub const LIBRARY_DB_UNAVAILABLE: ErrorCode = ErrorCode::declare("UZ-LIBRARY-006");

/// An operation failed for a reason the caller cannot act on and must not be told.
///
/// The code every crypto failure answers. A decrypt that fails because the tag
/// did not verify is indistinguishable, to a client, from one that failed
/// because the key was wrong — and saying which would be an oracle. The Zig
/// daemon reports `crypto_store` failures under this code for the same reason.
pub const INTERNAL_OPERATION_FAILED: ErrorCode = ErrorCode::declare("UZ-INTERNAL-003");

/// The datastore could not be reached, or the pool had nothing to give.
///
/// One code for both because a client cannot act on the difference — the
/// distinction that matters is operational, and `afd_db::Error` keeps it as
/// two variants for the operator while both answer here.
pub const INTERNAL_DB_UNAVAILABLE: ErrorCode = ErrorCode::declare("UZ-INTERNAL-001");

/// A statement reached Postgres and Postgres refused it.
pub const INTERNAL_DB_QUERY: ErrorCode = ErrorCode::declare("UZ-INTERNAL-002");

/// The schema ledger is not in a state this binary may migrate from.
///
/// `cmd/migrate.zig:50` and `cmd/preflight.zig:160` report the same code for
/// the same conditions: a lock nobody released, a version this binary does not
/// know, a migration that failed and left its failure row behind.
pub const STARTUP_MIGRATION_CHECK: ErrorCode = ErrorCode::declare("UZ-STARTUP-005");

/// Redis could not be reached, or did not answer in time.
///
/// `cmd/serve.zig` reports this when the dependency is absent at boot. A
/// request-path timeout answers the same code because the caller's situation is
/// identical — the datastore is not there — and the operator's distinction is
/// kept in `afd_redis::Error`'s variants rather than on the wire.
pub const STARTUP_REDIS_CONNECT: ErrorCode = ErrorCode::declare("UZ-STARTUP-004");

/// A knob is unset, blank, or not the shape this daemon can use.
///
/// `ERR_STARTUP_ENV_CHECK`. The one boot refusal that names something an
/// operator can fix without reaching a datastore, which is why it is its own
/// code rather than folded into the dependency failures beside it.
pub const STARTUP_ENV_CHECK: ErrorCode = ErrorCode::declare("UZ-STARTUP-001");

/// Postgres would not answer at boot.
///
/// `ERR_STARTUP_DB_CONNECT`. Distinct from [`INTERNAL_DB_UNAVAILABLE`], which
/// is a pool that would not answer a REQUEST: this one means the process never
/// started, and an orchestrator restarting it is the right response where a
/// client backing off is the right response to the other.
pub const STARTUP_DB_CONNECT: ErrorCode = ErrorCode::declare("UZ-STARTUP-003");
