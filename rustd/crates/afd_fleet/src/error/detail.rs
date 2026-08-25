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
