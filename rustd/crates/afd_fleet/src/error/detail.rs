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
