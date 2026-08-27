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
