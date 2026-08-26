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

/// A report arrived from a holder the fleet has already superseded.
///
/// `ERR_RUN_STALE_FENCING_TOKEN`. Referenced from the Zig registry, never
/// declared here as a new code (RULE ERR) — `error_registry.zig:206` owns the
/// value.
///
/// A 409, and the conflict is literal: two runners each believe they hold one
/// fleet, and the fence says which of them is right. The refused report writes
/// NOTHING — the flip, the settle and the tally all ride one guarded statement,
/// so a stale writer cannot land a partial finalize on the current holder's
/// run.
pub const RUN_STALE_FENCING_TOKEN: ErrorCode = ErrorCode::declare("UZ-RUN-005");

/// No lease with that id belongs to the presenting runner.
///
/// `ERR_RUN_LEASE_NOT_FOUND`. Referenced from the Zig registry
/// (`error_registry.zig:207`).
///
/// One code for two facts, deliberately: a lease that never existed and a lease
/// belonging to ANOTHER runner both answer this. The load is scoped by
/// `runner_id`, so a runner asking about a peer's lease gets the same 404 a
/// missing row gets — the scope IS the ownership check, and distinguishing the
/// two would turn this endpoint into an oracle for which lease ids are live.
pub const RUN_LEASE_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-RUN-006");

/// The runner is known and its administrative state bars the runner plane.
///
/// `ERR_RUN_ADMIN_STATE_BLOCKED`. Cordon, drain, revoke and delete all land
/// here, and this rejection is the ONLY channel by which a runner learns it is
/// out of service — the heartbeat reply is unconditionally `ok`.
pub const RUN_ADMIN_STATE_BLOCKED: ErrorCode = ErrorCode::declare("UZ-RUN-009");

/// The lease reached the hard ceiling on how long one run may take.
///
/// `ERR_RUN_LEASE_EXCEEDED_MAX_RUNTIME`. Referenced from the Zig registry
/// (`error_registry.zig:210`).
///
/// Distinct from [`RUN_LEASE_LOST`] even though both are 409s and both end the
/// run: this one says the runner did nothing wrong and its result is still
/// wanted — it stops the child and reports. Lost says the lease is somebody
/// else's now and the result will be refused. Collapsing them would throw away
/// a completed run's output at the cap.
pub const RUN_LEASE_EXCEEDED_MAX_RUNTIME: ErrorCode = ErrorCode::declare("UZ-RUN-010");

/// The lease moved to another runner before this renewal.
///
/// `ERR_RUN_LEASE_LOST`. Referenced from the Zig registry
/// (`error_registry.zig:211`).
///
/// Reached when the fence no longer holds or the row is no longer `active`, and
/// also when the lease row advanced but the affinity slot did not — a
/// half-applied renewal is reported LOST rather than renewed, because the slot
/// can be reclaimed before the deadline the reply would name.
pub const RUN_LEASE_LOST: ErrorCode = ErrorCode::declare("UZ-RUN-011");

/// The tenant's credit pool cannot fund another slice of this run.
///
/// `ERR_RUN_LEASE_RENEWAL_NO_CREDITS`. Referenced from the Zig registry
/// (`error_registry.zig:212`).
///
/// A 402, for the reason [`RUN_BUDGET_EXCEEDED`] is one: the runner classifies
/// a renew refusal by status AND code. The two 402s are different pools — this
/// is the TENANT's balance, that is the FLEET's own declared ceiling — and an
/// operator tops up for one and edits `TRIGGER.md` for the other.
pub const RUN_LEASE_RENEWAL_NO_CREDITS: ErrorCode = ErrorCode::declare("UZ-RUN-012");

/// No runner row matches an operator-supplied runner id.
///
/// `ERR_RUNNER_NOT_FOUND` in the Zig registry. Unlike
/// [`RUN_INVALID_RUNNER_TOKEN`], the caller has already authenticated on the
/// operator plane, so naming the missing resource is safe and actionable.
pub const RUNNER_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-RUN-014");

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

/// A fleet declared a credential the vault does not hold.
///
/// `ERR_AGENTSFLEET_CREDENTIAL_MISSING`. Reached from the lease path, where it
/// is LOGGED rather than answered: a fleet that names a credential nobody
/// stored cannot run, so the event is ended with a terminal row and the asking
/// runner is told there is no work. The code is what an operator correlates the
/// blocked event with.
pub const AGENTSFLEET_CREDENTIAL_MISSING: ErrorCode = ErrorCode::declare("UZ-AGT-003");

/// Untrusted Fleet Bundle bytes failed validation.
///
/// `ERR_FLEET_BUNDLE_INVALID` in the Zig registry. The detail kept by the
/// importing service identifies the violated bound without exposing content.
pub const FLEET_BUNDLE_INVALID: ErrorCode = ErrorCode::declare("UZ-BUNDLE-001");

/// No Fleet Bundle snapshot is stored under the requested content hash.
///
/// `ERR_FLEET_BUNDLE_NOT_FOUND`. Referenced from the Zig registry, never
/// declared here as a new code (RULE ERR) — `error_registry.zig:109` owns the
/// value.
///
/// Not an error the runner acts on by retrying. A bundle with no support files
/// stores no snapshot at all, so this is the ORDINARY answer for a skill-only
/// fleet: the runner proceeds with no support files rather than failing the
/// run. The same code answers a hash that names nothing, and the two are
/// deliberately indistinguishable — a runner holding a hash from its own lease
/// cannot tell them apart and does not need to, and distinguishing them would
/// make the endpoint an oracle for which snapshots exist.
pub const FLEET_BUNDLE_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-BUNDLE-002");

/// The Fleet Bundle snapshot store is unconfigured, or would not answer.
///
/// `ERR_FLEET_BUNDLE_STORAGE_UNAVAILABLE`. Referenced from the Zig registry
/// (`error_registry.zig:112`).
///
/// One code for both, because the runner acts identically on either: it is a
/// 503, the work is not refused, and the poll comes back. Which of the two it
/// was is an OPERATOR's question, and it is answered in the log beside the
/// request id — an unconfigured store names a knob nobody set, and a fetch
/// failure carries the store's own error as its source.
pub const FLEET_BUNDLE_STORAGE_UNAVAILABLE: ErrorCode = ErrorCode::declare("UZ-BUNDLE-005");

/// The instance is already serving as many requests as it admits.
///
/// `ERR_API_BACKPRESSURE`. A 429, and the one refusal in this registry that is
/// raised BEFORE anything about the caller is known — no credential has been
/// read, no handler has run. It says nothing about the request because at the
/// moment it is written nothing about the request has been looked at; what it
/// carries instead is `Retry-After`, which is the only actionable fact there is.
pub const API_BACKPRESSURE: ErrorCode = ErrorCode::declare("UZ-API-001");

/// The workspace has connected no integration under the requested name.
///
/// `ERR_CRED_INTEGRATION_NOT_CONNECTED` (`error_registry.zig:228`). A 404, and
/// the answer for BOTH "the vault holds no handle" and "the handle names a
/// connector this daemon does not carry" — a runner acts identically on either,
/// and telling them apart would make the mint an oracle for which connectors a
/// deployment ships.
pub const CRED_INTEGRATION_NOT_CONNECTED: ErrorCode = ErrorCode::declare("UZ-CRED-001");

/// This deployment has no credential broker wired.
///
/// `ERR_CRED_BROKER_NOT_CONFIGURED` (`error_registry.zig:229`). A 503, and an
/// OPERATOR's fault rather than a tenant's: the broker is a boot-wired
/// singleton, so its absence is a deployment that was never set up to mint.
pub const CRED_BROKER_NOT_CONFIGURED: ErrorCode = ErrorCode::declare("UZ-CRED-002");

/// The GitHub App installation is gone.
///
/// `ERR_GH_RECONNECT_REQUIRED` (`error_registry.zig:230`). Uninstalled or
/// revoked, so no token can be minted from it. A HUMAN's remedy — no amount of
/// retrying reconnects an App somebody removed — which is why it is its own
/// code rather than a mint failure.
pub const GH_RECONNECT_REQUIRED: ErrorCode = ErrorCode::declare("UZ-GH-001");

/// GitHub did not return an installation token this daemon would hand over.
///
/// `ERR_GH_MINT_FAILED` (`error_registry.zig:231`). One code for both retry
/// classes, deliberately: the runner reacts the same way to a vendor outage and
/// to a malformed exchange, and the class is the broker's own concern. It also
/// answers a token that came back reaching FURTHER than the fleet declared —
/// the exchange worked, the credential was discarded, and a runner cannot be
/// told the difference without being told what it nearly received.
pub const GH_MINT_FAILED: ErrorCode = ErrorCode::declare("UZ-GH-002");

/// The fleet holds no approved grant for the integration it asked to mint.
///
/// `ERR_GRANT_NOT_FOUND` (`error_registry.zig:195`). Absent, pending and
/// revoked all answer this: only an approved standing decision admits anything,
/// and a caller able to tell them apart would treat pending as a maybe.
pub const GRANT_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-GRANT-001");

/// A connector's OAuth exchange was rejected.
///
/// `ERR_CONNECTOR_OAUTH_EXCHANGE_FAILED` (`error_registry.zig:239`). The
/// refresh-grant connectors' answer to BOTH a dead refresh token and a failed
/// exchange, where GitHub has two codes for the same pair. That asymmetry is
/// the Zig's and is kept: a Zoho failure must never tell a runner to reconnect
/// a GitHub App, and the shared connector code is what stops it.
pub const CONNECTOR_OAUTH_EXCHANGE_FAILED: ErrorCode = ErrorCode::declare("UZ-CONN-006");

/// No human approved a repository-write gate for this event.
///
/// `ERR_REPAIR_WRITE_UNAPPROVED` (`error_registry.zig:199`). The run is not
/// refused — it continues read-only — so this is a refusal of the TOKEN and not
/// of the work.
pub const REPAIR_WRITE_UNAPPROVED: ErrorCode = ErrorCode::declare("UZ-REPAIR-010");

/// The fleet's declared reach no longer matches the approved card.
///
/// `ERR_REPAIR_BINDING_DRIFT` (`error_registry.zig:200`). Its own code rather
/// than an unapproved gate, because the remedy differs: an approval exists and
/// a human must be shown the reach the fleet declares NOW.
pub const REPAIR_BINDING_DRIFT: ErrorCode = ErrorCode::declare("UZ-REPAIR-011");

/// The approved write allowance is spent.
///
/// `ERR_REPAIR_SPEND_EXHAUSTED` (`error_registry.zig:202`). The one refusal in
/// this group that says the approval was real and was HONOURED — as far as it
/// went.
pub const REPAIR_SPEND_EXHAUSTED: ErrorCode = ErrorCode::declare("UZ-REPAIR-013");

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
    RUN_STALE_FENCING_TOKEN,
    RUN_LEASE_NOT_FOUND,
    RUN_ADMIN_STATE_BLOCKED,
    RUN_LEASE_EXCEEDED_MAX_RUNTIME,
    RUN_LEASE_LOST,
    RUN_LEASE_RENEWAL_NO_CREDITS,
    RUNNER_NOT_FOUND,
    RUN_BUDGET_EXCEEDED,
    AGENTSFLEET_CREDENTIAL_MISSING,
    FLEET_BUNDLE_INVALID,
    FLEET_BUNDLE_NOT_FOUND,
    FLEET_BUNDLE_STORAGE_UNAVAILABLE,
    API_BACKPRESSURE,
    CRED_INTEGRATION_NOT_CONNECTED,
    CRED_BROKER_NOT_CONFIGURED,
    GH_RECONNECT_REQUIRED,
    GH_MINT_FAILED,
    GRANT_NOT_FOUND,
    CONNECTOR_OAUTH_EXCHANGE_FAILED,
    REPAIR_WRITE_UNAPPROVED,
    REPAIR_BINDING_DRIFT,
    REPAIR_SPEND_EXHAUSTED,
];
