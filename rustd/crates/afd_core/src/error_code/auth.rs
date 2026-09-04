//! The codes that answer who a caller is, and what they may hold.
//!
//! `UZ-AUTH-*` for a credential and the device-flow login it may have come
//! from, `UZ-APIKEY-*` for the keys a tenant manages. One family in practice:
//! the login mints what the api-key surface then lists and revokes, and a
//! reader following a credential's life reads them in order.

use super::ErrorCode;

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

/// The caller proved who they are, and this object is not theirs.
///
/// `ERR_FORBIDDEN`. Distinct from [`AUTH_INSUFFICIENT_SCOPE`] on a seam a
/// caller can act on: that one says the credential lacks a capability and
/// names which, this one says the capability is held and the OBJECT belongs
/// to somebody else. Obtaining a scope fixes the first and can never fix the
/// second.
pub const AUTH_FORBIDDEN: ErrorCode = ErrorCode::declare("UZ-AUTH-001");

/// The device-flow session id names nothing this daemon holds.
///
/// `ERR_SESSION_NOT_FOUND`. One code for "never created", "evicted by the
/// five-minute time-to-live", and "not a version 7 identifier at all" — the
/// three are indistinguishable to a caller holding an id, and telling them
/// apart would make the poll an oracle for which login sessions are live.
pub const SESSION_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-AUTH-005");

/// The device-flow session's five-minute window closed before it was redeemed.
///
/// `ERR_SESSION_EXPIRED`. Terminal, and the remedy is a fresh `agentsfleet
/// login` rather than a retry of this one.
pub const SESSION_EXPIRED: ErrorCode = ErrorCode::declare("UZ-AUTH-006");

/// The six digits presented did not match the ones the browser showed.
///
/// `ERR_VERIFICATION_FAILED`. Retryable — deliberately, and up to a bounded
/// count — which is what separates it from [`SESSION_ABORTED`]: the attempt
/// that trips the ceiling answers that one instead, so a terminal informs the
/// command line to stop prompting rather than burning its own retries.
pub const VERIFICATION_FAILED: ErrorCode = ErrorCode::declare("UZ-AUTH-011");

/// The device-flow session was already redeemed.
///
/// `ERR_SESSION_CONSUMED`. A 410: the session existed and is gone, which is a
/// different fact from [`SESSION_NOT_FOUND`]'s 404 and reaches a caller only
/// after it has proven possession of the id.
pub const SESSION_CONSUMED: ErrorCode = ErrorCode::declare("UZ-AUTH-012");

/// The device-flow session was cancelled, superseded, or rate-limited.
///
/// `ERR_SESSION_ABORTED`. Terminal. The stored reason rides the log rather
/// than the wire, because the three causes have one remedy.
pub const SESSION_ABORTED: ErrorCode = ErrorCode::declare("UZ-AUTH-013");

/// A code was presented for a session no human has approved yet.
///
/// `ERR_SESSION_NOT_APPROVED`. A 409 rather than a 410: the session is still
/// approvable, so the caller waits rather than starting over.
pub const SESSION_NOT_APPROVED: ErrorCode = ErrorCode::declare("UZ-AUTH-014");

/// A second approval arrived for a session already past `pending`.
///
/// `ERR_SESSION_ALREADY_APPROVED`. The losing half of two dashboards racing
/// one Approve click; the winner's ciphertext stands.
pub const SESSION_ALREADY_APPROVED: ErrorCode = ErrorCode::declare("UZ-AUTH-015");

/// A presented public key is not one this daemon will store.
///
/// `ERR_INVALID_PUBLIC_KEY`. A SHAPE refusal and nothing more: the daemon
/// performs no elliptic-curve operation for device flow, so it can bound the
/// value's length and say nothing about whether it is a point on a curve.
pub const INVALID_PUBLIC_KEY: ErrorCode = ErrorCode::declare("UZ-AUTH-016");

/// The label a minted credential would carry is not a label.
///
/// `ERR_INVALID_TOKEN_NAME`.
pub const INVALID_TOKEN_NAME: ErrorCode = ErrorCode::declare("UZ-AUTH-017");

/// The verification code is not six decimal digits.
///
/// `ERR_INVALID_VERIFICATION_CODE`. Refused before any comparison, so a
/// malformed code costs no message authentication code computation and reveals
/// nothing about the stored one.
pub const INVALID_VERIFICATION_CODE: ErrorCode = ErrorCode::declare("UZ-AUTH-018");

/// The approval carried no ciphertext, or more than this daemon will relay.
///
/// `ERR_INVALID_CIPHERTEXT`. A bound rather than a decryption: the daemon
/// relays the envelope and never opens it.
pub const INVALID_CIPHERTEXT: ErrorCode = ErrorCode::declare("UZ-AUTH-019");

/// The approval carried no nonce, or one longer than the construction takes.
///
/// `ERR_INVALID_NONCE`.
pub const INVALID_NONCE: ErrorCode = ErrorCode::declare("UZ-AUTH-020");

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

/// No live command-line credential with that id belongs to this user.
///
/// `ERR_CLI_CREDENTIAL_NOT_FOUND`. One answer for three situations — never
/// existed, already revoked, or belongs to somebody else — for the reason
/// [`APIKEY_NOT_FOUND`] collapses its two: the revoke is owner-scoped in the
/// statement itself, so telling them apart would confirm another person's
/// credential to whoever guessed its identifier.
pub const AUTH_CLI_CREDENTIAL_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-AUTH-024");

/// The browser session could not be exchanged for a durable CLI credential.
///
/// Declared here because it is published (`api-reference/error-codes.mdx`) and
/// because the CLI emits it: `cli/src/commands/login-exchange.ts` answers with
/// this code when the exchange fails with no server code to relay — a transport
/// failure, an expired session. This daemon does not send it on any route
/// today; the registry of record still has to know every code a client can
/// show a person, or the client is emitting a code the product cannot explain.
pub const AUTH_CLI_CREDENTIAL_EXCHANGE_FAILED: ErrorCode = ErrorCode::declare("UZ-AUTH-025");

/// The tenant api-key resolved to a row that is no longer active.
///
/// `ERR_APIKEY_REVOKED`, and the tenant-key counterpart of
/// [`AUTH_CLI_CREDENTIAL_REVOKED`] — same reasoning, different family because
/// the two are revoked through different surfaces.
pub const APIKEY_REVOKED: ErrorCode = ErrorCode::declare("UZ-APIKEY-004");

/// No api-key with that id belongs to this tenant.
///
/// `ERR_APIKEY_NOT_FOUND`. The load is tenant-scoped, so a caller asking about
/// another tenant's key gets the same 404 a missing row gets — the scope IS the
/// ownership check, and distinguishing the two would make the endpoint an
/// oracle for which key identifiers exist.
pub const APIKEY_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-APIKEY-003");

/// A tenant already holds a key under that name.
///
/// `ERR_APIKEY_NAME_TAKEN`. Answered by the unique index rather than by a
/// pre-flight read: checking first would leave a window in which two concurrent
/// mints both pass and one loses at the insert anyway.
pub const APIKEY_NAME_TAKEN: ErrorCode = ErrorCode::declare("UZ-APIKEY-005");

/// The key was already revoked, so this call changed nothing.
///
/// `ERR_APIKEY_ALREADY_REVOKED`. Its own code rather than a silent success:
/// revocation is idempotent at the ROW level, and a caller still needs to know
/// whether their call was the one that did it.
pub const APIKEY_ALREADY_REVOKED: ErrorCode = ErrorCode::declare("UZ-APIKEY-006");

/// A revoked key cannot be brought back.
///
/// `ERR_APIKEY_READONLY_FIELD`. The only field this surface patches is
/// `active`, and only downward: a key whose digest may already be in somebody's
/// shell history must not become live again on one request.
pub const APIKEY_READONLY_FIELD: ErrorCode = ErrorCode::declare("UZ-APIKEY-007");

/// An active key must be revoked before it can be deleted.
///
/// `ERR_APIKEY_MUST_REVOKE_FIRST`. Two steps on purpose: revocation is the
/// reversible half — the row survives to explain itself — and a live credential
/// disappearing in one call leaves nothing to audit.
pub const APIKEY_MUST_REVOKE_FIRST: ErrorCode = ErrorCode::declare("UZ-APIKEY-008");
