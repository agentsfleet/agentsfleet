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

/// Identifier failed the canonical version-7 UUID shape (`id_format.zig`).
pub const UUIDV7_INVALID_ID_SHAPE: ErrorCode = ErrorCode::declare("UZ-UUIDV7-009");

/// Request body was malformed or violated a documented bound.
pub const INVALID_REQUEST: ErrorCode = ErrorCode::declare("UZ-REQ-001");

/// A stored envelope was malformed — wrong component length, or an unsupported version.
pub const VAULT_DATA_INVALID: ErrorCode = ErrorCode::declare("UZ-VAULT-001");

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

/// The principal authenticated, and is short a capability the route requires.
///
/// `ERR_INSUFFICIENT_SCOPE` in the Zig registry. A 403, never a 401: the caller
/// proved who they are and the answer is that who they are is not enough, so
/// re-authenticating cannot help and telling them to would send them in a
/// circle. The response names a scope from the route's own list, because a code
/// a caller can act on beats one they have to open a ticket about.
pub const AUTH_INSUFFICIENT_SCOPE: ErrorCode = ErrorCode::declare("UZ-AUTH-022");

/// No credential was presented, or the one presented proved nothing.
///
/// `ERR_UNAUTHORIZED`. A 401, and deliberately the SAME code for a missing
/// header, a malformed one, a well-formed credential no row matches, and a
/// token whose signature does not verify. Splitting those would tell a caller
/// which guess was closer, which is the one thing an unauthenticated caller
/// must not learn.
pub const AUTH_UNAUTHORIZED: ErrorCode = ErrorCode::declare("UZ-AUTH-002");

/// The token verified, and its expiry has passed.
///
/// `ERR_TOKEN_EXPIRED`. Distinct from [`AUTH_UNAUTHORIZED`] because it IS
/// actionable and leaks nothing: the holder already proved possession of a
/// validly-signed token, and the remedy — refresh it — is different from the
/// remedy for a token that never verified.
pub const AUTH_TOKEN_EXPIRED: ErrorCode = ErrorCode::declare("UZ-AUTH-003");

/// A credential could not be judged, because what judges it was unreachable.
///
/// `ERR_AUTH_UNAVAILABLE`. Never an authentication REJECTION, and the
/// distinction is load-bearing rather than tidy: the runner client counts
/// consecutive auth rejects toward a self-termination ceiling and resets that
/// counter on transport-class failures, so answering a Postgres blip with a
/// reject would walk a healthy fleet's runners to shutdown
/// (`runner_bearer.zig`'s `test "maps a lookup failure to UZ-AUTH-004"`).
pub const AUTH_UNAVAILABLE: ErrorCode = ErrorCode::declare("UZ-AUTH-004");

/// The command-line credential resolved to a row whose `revoked_at` is set.
///
/// `ERR_CLI_CREDENTIAL_REVOKED`. A revoked credential answers its own code
/// rather than collapsing into [`AUTH_UNAUTHORIZED`]: the holder is told the
/// credential is dead instead of retrying one that will never work again.
pub const AUTH_CLI_CREDENTIAL_REVOKED: ErrorCode = ErrorCode::declare("UZ-AUTH-023");

/// The tenant api-key resolved to a row that is no longer active.
///
/// `ERR_APIKEY_REVOKED`, and the tenant-key counterpart of
/// [`AUTH_CLI_CREDENTIAL_REVOKED`] — same reasoning, different family because
/// the two are revoked through different surfaces.
pub const APIKEY_REVOKED: ErrorCode = ErrorCode::declare("UZ-APIKEY-004");

/// No `fleet.runners` row matches the presented runner token.
///
/// `ERR_RUN_INVALID_RUNNER_TOKEN`. The runner plane's [`AUTH_UNAUTHORIZED`]:
/// a separate code because the runner client classifies its own plane's
/// rejections, and a tenant-plane 401 reaching it would be a category error.
pub const RUN_INVALID_RUNNER_TOKEN: ErrorCode = ErrorCode::declare("UZ-RUN-001");

/// The runner is known and its administrative state bars the runner plane.
///
/// `ERR_RUN_ADMIN_STATE_BLOCKED`. Cordon, drain, revoke and delete all land
/// here, and this rejection is the ONLY channel by which a runner learns it is
/// out of service — the heartbeat reply is unconditionally `ok`.
pub const RUN_ADMIN_STATE_BLOCKED: ErrorCode = ErrorCode::declare("UZ-RUN-009");

/// A fleet has reached a spend ceiling its own author declared.
///
/// `ERR_RUN_BUDGET_EXCEEDED`. Referenced from the Zig registry, never declared
/// here as a new code (RULE ERR) — `error_registry.zig:216` owns the value.
///
/// One code for both ceilings and both gates. `daily_dollars` and
/// `monthly_dollars` answer the same code because an operator acts identically
/// on either, and the issue-time refusal shares it with the mid-run kill at
/// `/renew` because they are the same fact observed at two moments. The verdict
/// that distinguishes them rides the log line, where it can be read without
/// making a client branch on it.
pub const RUN_BUDGET_EXCEEDED: ErrorCode = ErrorCode::declare("UZ-RUN-015");

/// The instance is already serving as many requests as it admits.
///
/// `ERR_API_BACKPRESSURE`. A 429, and the one refusal in this registry that is
/// raised BEFORE anything about the caller is known — no credential has been
/// read, no handler has run. It says nothing about the request because at the
/// moment it is written nothing about the request has been looked at; what it
/// carries instead is `Retry-After`, which is the only actionable fact there is.
pub const API_BACKPRESSURE: ErrorCode = ErrorCode::declare("UZ-API-001");

/// Every code this crate declares, in declaration order.
///
/// The exhaustive list the registry tests walk. A code added above without a
/// row here is invisible to the uniqueness and Zig-parity checks, which is why
/// `test_error_registry_unique` also asserts the count.
pub const REGISTRY: &[ErrorCode] = &[
    UUIDV7_INVALID_ID_SHAPE,
    INVALID_REQUEST,
    VAULT_DATA_INVALID,
    INTERNAL_OPERATION_FAILED,
    INTERNAL_DB_UNAVAILABLE,
    INTERNAL_DB_QUERY,
    STARTUP_MIGRATION_CHECK,
    STARTUP_REDIS_CONNECT,
    AUTH_INSUFFICIENT_SCOPE,
    AUTH_UNAUTHORIZED,
    AUTH_TOKEN_EXPIRED,
    AUTH_UNAVAILABLE,
    AUTH_CLI_CREDENTIAL_REVOKED,
    APIKEY_REVOKED,
    RUN_INVALID_RUNNER_TOKEN,
    RUN_ADMIN_STATE_BLOCKED,
    RUN_BUDGET_EXCEEDED,
    API_BACKPRESSURE,
];
