//! The sentences this plane's refusals carry.
//!
//! Every one is pinned byte-for-byte to the Zig handler it was ported from
//! — `session_helpers.zig`, `api_keys.zig`, `cli_credentials.zig` — because a
//! dashboard branches on some of them and a client prints the rest.

/// `problem_response.zig`'s `internalDbUnavailable` detail.
pub const DETAIL_DATABASE_UNAVAILABLE: &str = "Database unavailable";

/// `problem_response.zig`'s `internalDbError` detail.
pub const DETAIL_DATABASE_ERROR: &str = "Database error";

/// A queue outage, shaped like its database counterpart above.
///
/// Zig has no byte-identical original: its lease path collapses every Redis
/// failure to a no-work reply rather than surfacing one, so no `hx.fail` in
/// that family ever writes this sentence. It exists because a detail is not
/// optional here, and answering "Database unavailable" for a Redis outage
/// would send an operator to the wrong datastore. The CODE stays
/// `UZ-INTERNAL-003`, which is what the Zig assign path logs — no new registry
/// entry, so the ERROR REGISTRY gate does not fire.
pub const DETAIL_QUEUE_UNAVAILABLE: &str = "Queue unavailable";

/// `session_helpers.zig`'s refusal for a public key this daemon will not store.
pub const DETAIL_SESSION_PUBLIC_KEY: &str = "The supplied public_key is malformed";

/// Its refusal for a credential label that is not one.
pub const DETAIL_SESSION_TOKEN_NAME: &str = "token_name must be 1-64 characters of printable ASCII";

/// Its refusal for a relayed envelope that is absent or oversized.
pub const DETAIL_SESSION_CIPHERTEXT: &str = "ciphertext is missing, empty, or malformed";

/// Its refusal for a nonce that is absent or the wrong width.
pub const DETAIL_SESSION_NONCE: &str = "nonce is missing, empty, or the wrong length";

/// Its refusal for a code that is not six digits, raised before any compare.
pub const DETAIL_SESSION_CODE_SHAPE: &str = "verification_code must be exactly 6 ASCII digits";

/// Its refusal for a session id naming nothing this daemon holds.
pub const DETAIL_SESSION_MISSING: &str =
    "Session was not found. It may have expired or been invalidated";

/// Its refusal for a session whose five-minute window closed.
pub const DETAIL_SESSION_EXPIRED: &str = "Session expired";

/// Its refusal for a session already redeemed.
pub const DETAIL_SESSION_CONSUMED: &str = "This login session has already been consumed";

/// Its refusal for a session cancelled, superseded, or rate-limited.
pub const DETAIL_SESSION_ABORTED: &str = "This login session was aborted";

/// The verify dispatcher's terminal refusal on the attempt that trips the
/// ceiling.
///
/// Its own sentence rather than [`DETAIL_SESSION_ABORTED`], and the difference
/// is what the command line acts on: this one says stop prompting and log in
/// again, where the generic abort could as easily mean somebody clicked cancel.
pub const DETAIL_SESSION_RATE_LIMITED: &str = "Too many incorrect attempts — session aborted";

/// Its refusal for a code presented before any human approved the session.
pub const DETAIL_SESSION_NOT_APPROVED: &str = "Session not approved yet";

/// Its refusal for a second approval of one session.
pub const DETAIL_SESSION_ALREADY_APPROVED: &str = "This login session has already been approved";

/// Its refusal for an abort by an identity that does not hold the session.
pub const DETAIL_SESSION_NOT_OWNER: &str = "You do not own this login session";

/// The verify dispatcher's refusal for six digits that did not match.
pub const DETAIL_SESSION_CODE_REJECTED: &str = "Verification code did not match";

/// Its refusal for a name outside the character set or the bound.
pub const DETAIL_APIKEY_NAME: &str =
    "key_name must be 1-64 chars, alphanumeric + hyphen + underscore";

/// Its refusal for a description past its bound.
pub const DETAIL_APIKEY_DESCRIPTION: &str = "description must be <=256 chars";

/// Its refusal for an id naming no key this tenant holds.
pub const DETAIL_APIKEY_NOT_FOUND: &str = "API key not found";

/// Its refusal for a name this tenant already uses.
pub const DETAIL_APIKEY_NAME_TAKEN: &str = "Key name already exists in this tenant";

/// Its refusal for a revoke of an already-revoked key.
pub const DETAIL_APIKEY_ALREADY_REVOKED: &str = "API key is already revoked";

/// Its refusal for an attempt to bring a revoked key back.
pub const DETAIL_APIKEY_READONLY_FIELD: &str =
    "active cannot be set to true; mint a new key instead";

/// Its refusal for a delete of a key that is still live.
pub const DETAIL_APIKEY_MUST_REVOKE_FIRST: &str = "Active key must be revoked before deletion";

/// The command-line credential surface's refusal for a label it cannot store.
///
/// Deliberately NOT a grammar. The surface accepts any name a machine actually
/// has — spaces, apostrophes, any script — and refuses only a label that names
/// nothing once trimmed or does not fit the column. See
/// [`crate::cli_credential::MachineName`] for why the ASCII grammar this
/// replaced was the wrong rule in the wrong layer.
pub const DETAIL_CLI_CREDENTIAL_MACHINE_NAME: &str =
    "machine_name must be 1-64 characters and not blank";

/// Its refusal for an id naming no live credential this user holds.
///
/// One sentence for never-existed, already-revoked, and belongs-to-somebody-
/// else: the revoke is owner-scoped in the statement, and distinguishing them
/// would confirm another person's credential to whoever guessed its identifier.
pub const DETAIL_CLI_CREDENTIAL_NOT_FOUND: &str = "Command-line credential not found";

/// Its refusal for a proven subject with no `core.users` row behind it.
pub const DETAIL_CLI_CREDENTIAL_UNKNOWN_SUBJECT: &str = "Authenticated subject has no user record";

/// The create verb's refusal for a name carrying a character it will not store.
///
/// Control characters, bidirectional overrides and the line separators — the
/// set `workspaces/lifecycle.zig` refuses, because each of them lets a name
/// lie about itself in a list or a log line.
pub const DETAIL_WORKSPACE_NAME_INVALID: &str = "Workspace name contains unsupported characters";

/// Its refusal for a name past the cap.
///
/// The sentence says "characters" where the rule counts Unicode code points,
/// and stays that way: it is `lifecycle.zig`'s spelling, and a client may be
/// matching on it mid-cutover.
pub const DETAIL_WORKSPACE_NAME_TOO_LONG: &str = "Workspace name must be 128 characters or fewer";

/// Its refusal for a name this tenant already uses.
pub const DETAIL_WORKSPACE_NAME_EXISTS: &str = "A workspace with this name already exists";

/// Its refusal for a tenant claim naming no tenant row.
///
/// A 401 rather than a 403, as `lifecycle.zig` answers: the session itself is
/// stale — its tenant is gone — so re-authenticating is exactly the remedy.
pub const DETAIL_WORKSPACE_TENANT_VANISHED: &str = "Tenant on session does not exist";

/// The billing surface's report of a wallet row that is not there.
///
/// The em-dash sentence is `tenant_billing.zig`'s, byte for byte: the row is
/// written in the tenant-create transaction, so its absence is a bootstrap
/// invariant broken by surgery or a defect, and the sentence says whose problem
/// that is.
pub const DETAIL_BILLING_WALLET_MISSING: &str =
    "Tenant billing row missing — bootstrap invariant violated";

/// Its refusal for a charges cursor it never issued.
///
/// Lower-case and terse where the keyset cursor's refusals are sentences,
/// because this is `tenant_billing.zig`'s exact spelling and a cursor may be
/// judged by either binary mid-cutover.
pub const DETAIL_CHARGES_CURSOR_INVALID: &str = "invalid cursor";
