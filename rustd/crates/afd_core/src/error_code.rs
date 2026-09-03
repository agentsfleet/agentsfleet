//! The `UZ-*` codes a client matches on, declared once each.
//!
//! The Zig daemon single-sources every code in
//! `src/agentsfleetd/errors/error_registry.zig`, and that file stays the
//! registry of record for the whole product: `audits/error-codes.sh` greps it
//! alone, so a code declared anywhere else reads as an orphan at every use
//! site. This module is a CHECKED SUBSET of it — the codes the Rust port has
//! actually reached — not a second registry. `test_error_registry_matches_zig`
//! reads the Zig file and fails if a code here is spelled differently or is
//! absent there, so the two cannot drift apart silently while the port runs.
//!
//! Codes are added here as the milestone that emits them lands, never
//! speculatively: an unreferenced code is dead code that looks like coverage.

use std::fmt::{self, Display, Formatter};

use serde::Serialize;

/// A registry error code, spelled `UZ-<FAMILY>-<NNN>`.
///
/// Serialize-only by construction: the inner string is `'static` because every
/// code is declared in this module, so there is nothing for a deserializer to
/// borrow from or allocate into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ErrorCode(&'static str);

impl ErrorCode {
    /// Declares a registry code, rejecting a malformed spelling at compile time.
    ///
    /// # Panics
    /// During constant evaluation, if `code` is not spelled `UZ-<FAMILY>-<NNN>`.
    /// Every call site in this module is a `const` item, so a bad spelling is a
    /// build failure rather than a runtime surprise — the "correct by
    /// construction" route out of `M-PANIC-ON-BUG`.
    #[must_use]
    pub const fn declare(code: &'static str) -> Self {
        assert!(
            is_registry_spelling(code),
            "error code must be spelled UZ-<FAMILY>-<NNN>"
        );
        Self(code)
    }

    /// The code as it appears on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl Display for ErrorCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Whether `code` matches `UZ-<FAMILY>-<NNN>`: an upper-case alphanumeric
/// family, then exactly three digits.
///
/// Slice patterns rather than index arithmetic — the whole grammar is three
/// `match`es, and every bound is proven by the pattern instead of by a
/// comparison the reader has to check.
const fn is_registry_spelling(code: &str) -> bool {
    let [b'U', b'Z', b'-', tail @ ..] = code.as_bytes() else {
        return false;
    };
    let mut rest = tail;

    let mut family_len = 0usize;
    while let [head @ (b'A'..=b'Z' | b'0'..=b'9'), tail @ ..] = rest {
        let _ = head;
        family_len += 1;
        rest = tail;
    }
    if family_len == 0 {
        return false;
    }

    matches!(rest, [b'-', b'0'..=b'9', b'0'..=b'9', b'0'..=b'9'])
}

// One module per family, and the families are the ones the product already has:
// a request's own faults, who a caller is, the fleet plane, and a third party
// this daemon brokers with. Re-exported flat, so every call site still reads
// `error_code::AUTH_FORBIDDEN` and the split is invisible outside this file.
//
// `problem` is split the same way and in the same order, deliberately: the two
// tables are walked against each other by a test, and a family that lives in
// comparable files is a family somebody can actually compare.
mod auth;
mod fleet;
mod integration;
mod request;

pub use self::auth::*;
pub use self::fleet::*;
pub use self::integration::*;
pub use self::request::*;

/// Every code this crate declares, in declaration order.
///
/// The exhaustive list the registry tests walk. A code added above without a
/// row here is invisible to the uniqueness and Zig-parity checks, which is why
/// `test_error_registry_unique` also asserts the count.
pub const REGISTRY: &[ErrorCode] = &[
    UUIDV7_INVALID_ID_SHAPE,
    INVALID_REQUEST,
    PAYLOAD_TOO_LARGE,
    VAULT_DATA_INVALID,
    VAULT_DATA_TOO_LARGE,
    SECRET_NOT_FOUND,
    SECRET_REFERENCED_BY_MODEL_ENTRIES,
    SECRET_NAME_TAKEN,
    WEBHOOK_FLEET_NOT_FOUND,
    WEBHOOK_MALFORMED,
    WEBHOOK_SIGNATURE_INVALID,
    WEBHOOK_TIMESTAMP_STALE,
    WEBHOOK_CREDENTIAL_NOT_CONFIGURED,
    WEBHOOK_INSTALL_NOT_MAPPED,
    WEBHOOK_SUBSCRIPTION_NOT_FOUND,
    WEBHOOK_PAYLOAD_TOO_LARGE,
    SCHEDULE_NOT_FOUND,
    SCHEDULE_LIMIT_REACHED,
    SCHEDULE_KEY_TAKEN,
    SCHEDULE_SYNCING,
    APPROVAL_NOT_FOUND,
    APPROVAL_ALREADY_RESOLVED,
    PREF_KEY_UNKNOWN,
    PREF_VALUE_TOO_LARGE,
    WORKSPACE_NAME_EXISTS,
    LIBRARY_CURSOR_MALFORMED,
    LIBRARY_CURSOR_MISMATCH,
    LIBRARY_INPUT_OUT_OF_BOUNDS,
    LIBRARY_DB_UNAVAILABLE,
    INTERNAL_OPERATION_FAILED,
    INTERNAL_DB_UNAVAILABLE,
    INTERNAL_DB_QUERY,
    STARTUP_ENV_CHECK,
    STARTUP_DB_CONNECT,
    STARTUP_MIGRATION_CHECK,
    STARTUP_REDIS_CONNECT,
    AUTH_INSUFFICIENT_SCOPE,
    AUTH_UNAUTHORIZED,
    AUTH_TOKEN_EXPIRED,
    AUTH_FORBIDDEN,
    SESSION_NOT_FOUND,
    SESSION_EXPIRED,
    VERIFICATION_FAILED,
    SESSION_CONSUMED,
    SESSION_ABORTED,
    SESSION_NOT_APPROVED,
    SESSION_ALREADY_APPROVED,
    INVALID_PUBLIC_KEY,
    INVALID_TOKEN_NAME,
    INVALID_VERIFICATION_CODE,
    INVALID_CIPHERTEXT,
    INVALID_NONCE,
    AUTH_UNAVAILABLE,
    AUTH_CLI_CREDENTIAL_REVOKED,
    AUTH_CLI_CREDENTIAL_NOT_FOUND,
    APIKEY_REVOKED,
    APIKEY_NOT_FOUND,
    APIKEY_NAME_TAKEN,
    APIKEY_ALREADY_REVOKED,
    APIKEY_READONLY_FIELD,
    APIKEY_MUST_REVOKE_FIRST,
    RUN_INVALID_RUNNER_TOKEN,
    RUN_STALE_FENCING_TOKEN,
    RUN_LEASE_NOT_FOUND,
    RUN_ADMIN_STATE_BLOCKED,
    RUN_LEASE_EXCEEDED_MAX_RUNTIME,
    RUN_LEASE_LOST,
    RUN_LEASE_RENEWAL_NO_CREDITS,
    RUNNER_NOT_FOUND,
    RUNNER_MUST_REVOKE_FIRST,
    RUN_BUDGET_EXCEEDED,
    RUN_SELFTEST_REFUSED,
    AGENTSFLEET_CREDENTIAL_MISSING,
    AGENTSFLEET_NAME_EXISTS,
    AGENTSFLEET_INVALID_CONFIG,
    AGENTSFLEET_NOT_FOUND,
    AGENTSFLEET_ALREADY_TERMINAL,
    AGENTSFLEET_NAME_MISMATCH,
    AGENTSFLEET_INSTALL_ROLLED_BACK,
    AGENTSFLEET_SOURCE_STALE,
    AGENTSFLEET_PAUSED_INGRESS,
    EVENT_NOT_FOUND,
    MEM_AGENTSFLEET_NOT_FOUND,
    MEM_UNAVAILABLE,
    MEM_ENTRY_NOT_FOUND,
    FLEET_BUNDLE_INVALID,
    FLEET_BUNDLE_NOT_FOUND,
    FLEET_BUNDLE_SECRETS_MISSING,
    FLEET_BUNDLE_FETCH_FAILED,
    FLEET_BUNDLE_STORAGE_UNAVAILABLE,
    PROVIDER_SECRET_REF_REQUIRED,
    PROVIDER_SECRET_NOT_FOUND,
    PROVIDER_SECRET_DATA_MALFORMED,
    PROVIDER_MODEL_NOT_IN_CATALOGUE,
    PROVIDER_BASE_URL_INVALID,
    PROVIDER_MODEL_NOT_FOUND,
    PROVIDER_MODEL_IN_USE,
    PROVIDER_MODEL_EXISTS,
    PROVIDER_PLATFORM_KEY_MISSING,
    TENANT_NO_PRIMARY_WORKSPACE,
    MODELS_DELETE_ACTIVE,
    MODELS_SECRET_NOT_FOUND,
    MODELS_DUPLICATE_ENTRY,
    MODELS_ENTRY_NOT_FOUND,
    CATALOG_NOT_FOUND,
    CATALOG_PUBLISH_WITHOUT_BUNDLE,
    CATALOG_DELETE_PUBLISHED,
    CATALOG_ID_COLLISION,
    CATALOG_ROW_STALE,
    API_BACKPRESSURE,
    SSE_STREAM_CAP,
    CRED_INTEGRATION_NOT_CONNECTED,
    CRED_BROKER_NOT_CONFIGURED,
    GH_RECONNECT_REQUIRED,
    GH_MINT_FAILED,
    GRANT_NOT_FOUND,
    GRANT_REVOKE_NOT_FOUND,
    CONNECTOR_OAUTH_EXCHANGE_FAILED,
    CONNECTOR_NOT_CONFIGURED,
    CONNECTOR_STATE_INVALID,
    CONNECTOR_VENDOR_DEADLINE,
    CONNECTOR_UNKNOWN,
    REPAIR_WRITE_UNAPPROVED,
    REPAIR_BINDING_DRIFT,
    REPAIR_SPEND_EXHAUSTED,
];
