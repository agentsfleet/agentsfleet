//! The words `core.fleet_events` stores in its `status` and `failure_label`
//! columns.
//!
//! # Why here and not in `afd_wire`
//!
//! `afd_wire` is a byte-exact port of the frozen `/v1/runners` contract, and it
//! depends on nothing but serde ON PURPOSE — its own module documentation says
//! so, because a validated newtype reaching that layer would break the fixture
//! byte-equality the crate exists to guarantee. Neither of these columns is on
//! that wire: `EventEnvelope` carries `event_type` and not `status`, so putting
//! stored vocabulary there would have widened a frozen contract crate into a
//! general dictionary.
//!
//! They belong in the value layer instead, beside [`crate::error_code`] — which
//! is the same shape of thing, a registry of strings three planes agree on.
//! `event_type` stays in `afd_wire` because it genuinely IS on the wire.
//!
//! # Why the vocabulary is shared at all
//!
//! Three planes read these columns: the runner reports into `status`, the
//! daemon's gates refuse into it, and the operator surface renders both. Before
//! this module they were declared in `afd_fleet::sql::event` and copied into
//! `afd_approval` — a second and third spelling at a second and third write
//! site, which is a row one plane writes that another cannot recognise.

/// The stored spellings of `core.fleet_events.status`.
pub mod status {
    /// The status an event opens in.
    ///
    /// `event_rows.zig`'s `received`. The report flips it to [`PROCESSED`] or
    /// [`FLEET_ERROR`], and the gate pass to [`GATE_BLOCKED`] — all three read
    /// this spelling as the status they are guarded against, so it is declared
    /// once (RULE UFS).
    pub const RECEIVED: &str = "received";

    /// The status a gate refusal flips an event INTO.
    ///
    /// Daemon-side and terminal: a runner never reports it. Paired with
    /// [`RECEIVED`] because the two are the halves of one predicate — the
    /// failure update guards on the second and writes the first.
    pub const GATE_BLOCKED: &str = "gate_blocked";

    /// The status a clean run ends in.
    ///
    /// `event_rows.zig`'s `STATUS_PROCESSED`.
    pub const PROCESSED: &str = "processed";

    /// The status a failed run ends in.
    ///
    /// `event_rows.zig`'s `STATUS_FLEET_ERROR`. Runner-reported, unlike
    /// [`GATE_BLOCKED`]: the daemon refuses at a gate, and the runner reports a
    /// failure it observed.
    pub const FLEET_ERROR: &str = "fleet_error";
}

/// Why a gate refused an event, as `core.fleet_events.failure_label`.
///
/// One ownership site (RULE UFS) — the webhook path, the steer path and the
/// dashboard all read these strings, and a second spelling at a second write
/// site is a label that silently stops matching. `balance_exhausted`'s spelling
/// is pinned by `billing_and_provider_keys.md`.
pub mod label {
    /// The tenant's credit pool cannot cover the estimate.
    pub const BALANCE_EXHAUSTED: &str = "balance_exhausted";

    /// The workspace resolves to no tenant — a broken foreign key, not a blip.
    pub const TENANT_RESOLVE_FAILED: &str = "tenant_resolve_failed";

    /// The fleet's own declared ceiling is reached.
    ///
    /// Spelled identically to the runner-reported `budget_breach` failure
    /// class, which carries the same verdict for the mid-run kill — one label
    /// for two gates, so an operator greps one string whether the run was
    /// refused at issue or stopped in flight.
    pub const BUDGET_BREACH: &str = "budget_breach";

    /// A declared credential has no vault row.
    pub const SECRET_MISSING: &str = "secret_missing";

    /// A human refused the action.
    pub const APPROVAL_DENIED: &str = "approval_denied";

    /// A human was asked and the deadline passed.
    pub const APPROVAL_EXPIRED: &str = "approval_expired";

    /// The event names a type this daemon has no execution path for.
    ///
    /// New in the Rust port, and it has no Zig counterpart because the Zig
    /// carries `event_type` as a string all the way to the runner and never has
    /// to decide whether it can spell it. Here the wire type is a closed enum,
    /// so a producer from a newer build is a case that must be named — and
    /// naming it is the point: the alternative spellings available were all
    /// lies about what happened, and an operator reading
    /// `tenant_resolve_failed` on this row would go and look at billing.
    pub const EVENT_TYPE_UNSUPPORTED: &str = "event_type_unsupported";

    /// A write binding could not be turned into rules that bound anything.
    ///
    /// Also new, and also a fleet author's mistake rather than an operational
    /// fault: no repair branch was authorised, no base was named, or the
    /// binding covers more than the one repository the locked rules bound.
    /// Distinct from [`SECRET_MISSING`] because nothing is missing from the
    /// vault — the fleet's own configuration cannot be enforced.
    pub const BINDING_UNENFORCEABLE: &str = "binding_unenforceable";
}
