//! What a caller is TOLD, as distinct from what went wrong.
//!
//! Every sentence here is client-visible, so each is pinned byte-for-byte
//! against the `problem_response.zig` or handler original it replaces: parity
//! in this milestone is behavioural, and what a caller reads is behaviour
//! (RULE UFS). They live apart from [`super::ErrorKind`] because the two answer
//! different questions — that one is what happened, this is what we say — and
//! because a sentence changing is a wire change while a kind changing is not.

/// `register.zig`'s refusal when `host_id` is absent or too long.
///
/// Client-visible, so it is pinned byte-for-byte: parity in this milestone is
/// behavioural, and what a caller reads is behaviour (RULE UFS).
pub const DETAIL_HOST_ID_BOUNDS: &str = "host_id must be 1-256 chars";

/// `register.zig`'s refusal for a malformed registry allowlist entry.
pub const DETAIL_REGISTRY_ALLOWLIST: &str = "registry_allowlist entries must be host[:port] names";

/// `self.zig`'s refusal when the token authenticated and the row is gone.
pub const DETAIL_RUNNER_NOT_FOUND: &str = "runner not found";

/// `problem_response.zig`'s `internalDbUnavailable` detail.
pub const DETAIL_DATABASE_UNAVAILABLE: &str = "Database unavailable";

/// `problem_response.zig`'s `internalDbError` detail.
pub const DETAIL_DATABASE_ERROR: &str = "Database error";

/// An event on the stream this daemon cannot execute.
///
/// The caller is a runner asking for work, so what it reads says nothing about
/// its own request — the missing field goes to the log, where an operator can
/// correlate it with the producer that wrote the entry.
pub const DETAIL_EVENT_MALFORMED: &str = "leased event malformed";

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

/// `register.zig`'s `internalOperationError` detail.
///
/// Reached only from the enrolment path: it is the one verb that mints an
/// identifier a client is waiting on, and every other mint in this crate is
/// best-effort and never surfaces.
pub const DETAIL_REGISTRATION_FAILED: &str = "runner registration failed";

/// A lease whose tenant's provider could not be resolved.
///
/// Zig has no byte-identical original, for the same reason
/// [`DETAIL_QUEUE_UNAVAILABLE`] does not: `service_billing.zig` answers a
/// provider-resolution failure with a no-work reply and a `warn`, so no
/// `hx.fail` in that family ever writes a sentence for it. It exists because a
/// detail is not optional here, and it deliberately says nothing about WHICH
/// part of the configuration is broken — the caller is a runner asking for
/// work, and the tenant's vault layout is not its business. The operator gets
/// the field name in the log.
pub const DETAIL_PROVIDER_UNRESOLVED: &str = "provider unresolved";

/// A fleet naming a credential the vault does not hold.
///
/// Pinned to the Zig registry entry for `UZ-AGT-003`, which is the sentence an
/// operator reads on every other surface that raises this code. The lease path
/// itself never writes it — it ends the event and answers no-work — but a
/// detail is not optional, and answering a DIFFERENT sentence for the same code
/// on one surface is how a runbook stops matching what the product says.
pub const DETAIL_CREDENTIAL_MISSING: &str =
    "A required credential is not in the vault. Add it with: `agentsfleet secret create <NAME>`";

/// A stored credential body that is not an addressable JSON object.
///
/// The `UZ-VAULT-001` registry TITLE, byte-identical, rather than a sentence
/// invented here: the tenant plane's create/replace routes answer this code for
/// the same fact, and the lease path meeting it in stored data is that
/// validation having been bypassed — `storeJsonPlaintext` skips the shape gate
/// by design, so a malformed body can reach the vault. One fact, one sentence.
pub const DETAIL_VAULT_DATA_INVALID: &str = "Secret data must be a non-empty JSON object";

/// A fleet's stored configuration could not be read.
///
/// No `problem_response.zig` sentence to copy: the Zig meets this failure
/// inside its claim, where it returns an error the fleet loop LOGS and the
/// runner is simply told there is no work. Here the same failure can reach a
/// caller, so it needs a sentence — and this one names the stored document
/// rather than the request, because the runner did nothing wrong and the fix
/// is in the fleet.
pub const DETAIL_CONFIG_UNREADABLE: &str = "fleet configuration unreadable";

/// A gate reference that could not be written.
///
/// Unreachable for the shape the gate stores — a record of a string and an
/// integer — and present because the alternative is swallowing a failure that
/// would leave a parked event unable to find its own gate. Never rendered: the
/// lease path answers no-work rather than surfacing this.
pub const DETAIL_GATE_REFERENCE_UNWRITABLE: &str = "approval gate reference unwritable";

/// An approved reach that could not be recorded.
///
/// The sibling of [`DETAIL_GATE_REFERENCE_UNWRITABLE`], and unreachable for the
/// same kind of reason: the recorded binding is a list of strings, an enum and
/// an optional string, none of which has a serializer failure to reach. Present
/// because the alternative is a gate row whose `stated_binding` is `NULL` — and
/// the write mint refuses those, so swallowing it would turn an impossible
/// failure into an approval nobody can spend. Never rendered.
pub const DETAIL_GATE_BINDING_UNWRITABLE: &str = "approval gate binding unwritable";

/// `service_report.zig`'s refusal when the presenting holder has been
/// superseded.
///
/// It names the outcome — the current holder's result wins — because that is
/// the fact the runner acts on: it stops retrying and discards its own result,
/// rather than backing off and re-reporting into a lease it no longer holds.
pub const DETAIL_STALE_FENCE: &str = "Lease superseded by a newer holder; report rejected";

/// `service_renew.zig`'s refusal when no lease with that id is the caller's.
///
/// Deliberately says nothing about WHICH of the two happened — no such lease,
/// or somebody else's lease. The load is scoped by runner, so this sentence is
/// all either case can honestly claim to know, and a sharper one would make the
/// endpoint an oracle for live lease ids.
pub const DETAIL_LEASE_NOT_FOUND: &str = "No lease matches this lease_id for the runner";

/// `service_renew.zig`'s refusal when the lease moved on before this renewal.
///
/// One sentence for what the Zig spells two ways — "no longer active; reclaimed
/// or already reported" at the status check, and "reassigned before this
/// renewal" after the atomic extend. Both are the same fact observed a moment
/// apart, and the runner's remedy is identical either way: terminate the child.
/// Two sentences would suggest a distinction it could act on and cannot.
pub const DETAIL_LEASE_LOST: &str = "Lease was reassigned before this renewal; terminate the child";

/// `service_renew.zig`'s refusal at the hard runtime ceiling.
pub const DETAIL_LEASE_MAX_RUNTIME: &str = "Lease reached the hard max runtime; not renewed";

/// `service_renew.zig`'s refusal when the TENANT's credit pool is spent.
///
/// Distinct from [`DETAIL_BUDGET_EXHAUSTED`] beside it, and the two sentences
/// are the only thing that tells an operator which pool to look at: this one is
/// topped up, that one is edited in `TRIGGER.md`.
pub const DETAIL_RENEWAL_NO_CREDITS: &str =
    "Tenant balance can no longer fund this run; not renewed";

/// `service_renew.zig`'s refusal when the FLEET's own ceiling is reached.
pub const DETAIL_BUDGET_EXHAUSTED: &str = "Fleet budget exhausted for this window; not renewed";

/// `bundles.zig`'s answer when nothing is stored under a content hash.
///
/// Reads as a statement of fact rather than as a fault, because it is one: a
/// bundle with no support files stores no snapshot, so a runner meeting this
/// proceeds with none. The sentence is the Zig's verbatim.
pub const DETAIL_BUNDLE_NOT_FOUND: &str = "no snapshot stored for this content hash";

/// `bundles.zig`'s answer when snapshot storage is not configured at all.
pub const DETAIL_BUNDLE_STORAGE_UNAVAILABLE: &str = "Fleet Bundle snapshot storage is unavailable";

/// `bundles.zig`'s answer when the store was reached and would not serve.
///
/// Distinct from [`DETAIL_BUNDLE_STORAGE_UNAVAILABLE`] beside it, and the Zig
/// draws the same line under one code: an operator reading the first goes and
/// sets four knobs, and reading the second goes and looks at the bucket. The
/// runner cannot act on the difference and is not asked to — both are 503s it
/// re-polls past.
pub const DETAIL_BUNDLE_FETCH_FAILED: &str = "Fleet Bundle snapshot fetch failed";

/// `credentials_mint.zig`'s `S_INTEGRATION_NOT_CONNECTED`.
///
/// Answers BOTH a workspace that connected nothing under this name and a handle
/// naming a connector this registry does not carry. One sentence for both, and
/// deliberately: a runner acts identically on either, and telling them apart
/// would make the mint an oracle for which connectors a deployment ships.
pub const DETAIL_INTEGRATION_NOT_CONNECTED: &str = "Integration not connected for this workspace";

/// `credentials_mint.zig`'s broker-absent sentence.
///
/// An OPERATOR's fault, and the sentence says so: no tenant action reaches it,
/// because what is missing is this deployment's own platform credential.
pub const DETAIL_MINT_UNCONFIGURED: &str = "This deployment isn't set up to mint credentials yet";

/// `credentials_mint.zig`'s GitHub reconnect sentence.
pub const DETAIL_GITHUB_RECONNECT: &str = "GitHub App installation needs reconnect";

/// `credentials_mint.zig`'s `S_MINT_FAILED`.
pub const DETAIL_MINT_FAILED: &str = "Credential mint failed";

/// `credentials_mint.zig`'s `S_CONNECTOR_RECONNECT`.
///
/// Provider-NEUTRAL on purpose. A Zoho refresh that failed must never tell a
/// runner to reconnect a GitHub App, which is what a shared sentence across the
/// two families would eventually do.
pub const DETAIL_CONNECTOR_RECONNECT: &str =
    "Connector authorization expired — reconnect the integration";

/// `credentials_mint.zig`'s `S_CONNECTOR_MINT_FAILED`.
pub const DETAIL_CONNECTOR_MINT_FAILED: &str = "Connector token refresh failed";

/// `credentials_mint.zig`'s `S_GRANT_REQUIRED`.
pub const DETAIL_GRANT_REQUIRED: &str =
    "No approved integration grant for this fleet and integration";

/// `credentials_mint.zig`'s `S_WRITE_UNAPPROVED`.
pub const DETAIL_WRITE_UNAPPROVED: &str =
    "No approved repository-write gate for this lease's event";

/// `credentials_mint.zig`'s `S_BINDING_DRIFT`.
pub const DETAIL_BINDING_DRIFT: &str =
    "Fleet repository binding changed since the approval was answered";

/// `credentials_mint.zig`'s `S_WRITE_SPEND_EXHAUSTED`.
pub const DETAIL_WRITE_SPEND_EXHAUSTED: &str =
    "Approved write-credential request allowance is exhausted";

// ── The device-flow login surface ────────────────────────────────────────
//
// Every sentence below is `session_helpers.zig`'s `failFromStoreError` mapping
// or the verify dispatcher's, pinned byte-for-byte. Where the Zig writes two
// spellings for one code — the poll path's "Session already consumed" against
// the store path's fuller sentence — the fuller one wins and the short one
// goes: two sentences for one code is the drift RULE UFS names, and a caller
// matching on the code cannot act on which handler it came from.

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

// ── The tenant api-key lifecycle ─────────────────────────────────────────
//
// Pinned to `api_keys/tenant.zig`'s own sentences. The lifecycle refusals are
// the ones a dashboard renders directly, so each says what to do next rather
// than what went wrong.

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
