//! What a metered slice costs — rate resolution and the charge arithmetic.
//!
//! Split from `tenant_billing.zig` at the 350-line cap (RULE FLL). The seam is by
//! question: that file owns the tenant's LEDGER (balance, grants, debits,
//! exhaustion) and the rate CONSTANTS, which `audits/cross-tier-rates.sh` pins to
//! that exact path across four files. This one owns turning a `(provider, model,
//! posture, elapsed, tokens)` tuple into a number.
//!
//! The dependency runs one way — this module reads the ledger's constants, the
//! ledger never reads this — so the split cannot become a cycle.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;

const billing = @import("tenant_billing.zig");
const model_rate_cache = @import("model_rate_cache.zig");
const revision_state = @import("model_catalogue_revision.zig");

const Posture = billing.Posture;
const RUN_NANOS_PER_SEC = billing.RUN_NANOS_PER_SEC;

/// Unit divisors for the per-slice charge math: ms→s and per-million-tokens.
const MS_PER_SEC: i64 = 1000;
const TOKENS_PER_MTOK: i64 = 1_000_000;

/// under both postures. `RUN_NANOS_PER_SEC` is per-second; the ms→s division
/// uses the same `@divTrunc` discipline as the per-mtok token math. i64-safe:
/// `elapsed_ms` is bounded by the lease's MAX_RUNTIME_MS, so `elapsed_ms *
/// RUN_NANOS_PER_SEC` stays well inside i64. At lease issue `elapsed_ms` is 0,
/// so the run fee is 0 and only the token estimate (platform) is charged.
fn runFee(elapsed_ms: i64) i64 {
    return @divTrunc(elapsed_ms * RUN_NANOS_PER_SEC, MS_PER_SEC);
}

/// Per-slice stage charge: a run fee for `elapsed_ms` of active runtime plus,
/// under platform posture, the per-token model cost of the token counts resolved
/// from the catalogue (all in nanos). Under self_managed the run fee is the whole
/// charge — token cost lands on the user's own provider bill.
///
/// Takes a connection because the platform branch prices against the catalogue
/// generation this connection observes; free-trial and self-managed resolve with
/// no statement at all. An error means the rate could not be established — the
/// caller decides its own posture for that and must not substitute a guess.
///
/// `error.ModelNotPriced` under platform when the catalogue authoritatively has
/// no such model. The upstream validators (the tenant-provider PUT and the
/// install-skill frontmatter check) reject an uncatalogued model, but the
/// catalogue can move after they ran: an admin DELETE of a non-default row
/// leaves any tenant still naming that model reaching this resolve and getting
/// a database answer of "no row". That is an operational state, not a
/// programmer bug — a panic here aborted the whole replica for one fleet's
/// stale model, on every replica that picked the fleet up. The error lets each
/// caller take its documented posture: the lease-estimate gate fails OPEN (an
/// estimate is not a charge), renew/settle fails CLOSED.
pub fn computeStageCharge(
    conn: *pg.Conn,
    provider: []const u8,
    posture: Posture,
    model: []const u8,
    elapsed_ms: i64,
    input_tokens: u32,
    cached_input_tokens: u32,
    output_tokens: u32,
) !i64 {
    return computeStageChargeAt(conn, provider, posture, model, elapsed_ms, input_tokens, cached_input_tokens, output_tokens, clock.nowMillis());
}

/// `computeStageCharge` at an injected clock — pub so the DB-backed sibling
/// suite (`tenant_billing_edge_test.zig`) can price post-trial states, the
/// uncatalogued-model refusal included, while the promotional window is still
/// open on the real clock.
pub fn computeStageChargeAt(
    conn: *pg.Conn,
    provider: []const u8,
    posture: Posture,
    model: []const u8,
    elapsed_ms: i64,
    input_tokens: u32,
    cached_input_tokens: u32,
    output_tokens: u32,
    now_ms: i64,
) !i64 {
    const rates = (try resolveRenewSliceRates(conn, provider, posture, model, now_ms)) orelse
        return error.ModelNotPriced;
    return sliceCharge(rates, elapsed_ms, @as(i64, input_tokens), @as(i64, cached_input_tokens), @as(i64, output_tokens));
}

/// The four per-unit rates a renewal/settle slice meters at. Resolved once in
/// Zig and passed to the renewal CTE as params, so the SQL applies the SAME
/// rates `computeStageCharge` does — SQL==Zig holds by construction, not by
/// hand-copying the rate table into SQL.
pub const SliceRates = struct {
    run_nanos_per_sec: i64,
    input_nanos_per_mtok: i64,
    cached_input_nanos_per_mtok: i64,
    output_nanos_per_mtok: i64,
};

/// The two branches that price without consulting the catalogue at all:
/// free-trial (`now_ms < billing.FREE_TRIAL_END_MS`) → all zero (a metered run charges
/// 0); self_managed → run rate only (token tiers 0, recorded-not-charged).
///
/// `null` means "the catalogue is required", i.e. platform posture outside the
/// trial. Split out so those two paths cost no statement and stay provable
/// without a database — and so the free-trial short-circuit is visibly ahead of
/// the posture switch, which is what makes a missing model harmless during the
/// trial window.
fn sliceRatesWithoutCatalogue(posture: Posture, now_ms: i64) ?SliceRates {
    if (billing.isFreeTrialActive(now_ms)) return SliceRates{ .run_nanos_per_sec = 0, .input_nanos_per_mtok = 0, .cached_input_nanos_per_mtok = 0, .output_nanos_per_mtok = 0 };
    return switch (posture) {
        .self_managed => SliceRates{ .run_nanos_per_sec = RUN_NANOS_PER_SEC, .input_nanos_per_mtok = 0, .cached_input_nanos_per_mtok = 0, .output_nanos_per_mtok = 0 },
        .platform => null,
    };
}

/// Resolve the four slice rates, pricing the platform branch against the
/// catalogue generation `conn` observes.
///
/// The generation is read here rather than accepted as a parameter, for the same
/// reason `secret_reference_txn.begin` derives its tenant: a caller that can
/// supply the generation is a caller that can supply the wrong one, and the
/// failure that produces — billing a slice at a generation the catalogue has
/// moved past — is silent.
///
/// `null` means the catalogue authoritatively has no row for `(provider, model)`.
/// An ERROR means the rate could not be established, which is a different answer:
/// the caller fails closed rather than pricing from whatever was cached. The
/// (provider, model) pair keys the rate row — same model, two providers, two
/// rates.
pub fn resolveRenewSliceRates(
    conn: *pg.Conn,
    provider: []const u8,
    posture: Posture,
    model: []const u8,
    now_ms: i64,
) !?SliceRates {
    if (sliceRatesWithoutCatalogue(posture, now_ms)) |rates| return rates;

    const revision = try revision_state.read(conn);
    const rate = (try model_rate_cache.rateAtRevision(conn, revision, provider, model)) orelse return null;
    return SliceRates{
        .run_nanos_per_sec = RUN_NANOS_PER_SEC,
        .input_nanos_per_mtok = rate.input_nanos_per_mtok,
        .cached_input_nanos_per_mtok = rate.cached_input_nanos_per_mtok,
        .output_nanos_per_mtok = rate.output_nanos_per_mtok,
    };
}

/// Apply slice rates to a set of deltas — the exact arithmetic the renewal CTE
/// reproduces in SQL (per-tier `@divTrunc(rate*Δ, 1e6)` + ms→s `@divTrunc(Δt*run,
/// 1000)`; Postgres bigint `/` truncates toward zero, matching for Δ≥0). This is
/// the reference the SQL==Zig pin test asserts against.
pub fn sliceCharge(rates: SliceRates, elapsed_ms: i64, d_input: i64, d_cached: i64, d_output: i64) i64 {
    return @divTrunc(elapsed_ms * rates.run_nanos_per_sec, MS_PER_SEC) +
        @divTrunc(rates.input_nanos_per_mtok * d_input, TOKENS_PER_MTOK) +
        @divTrunc(rates.cached_input_nanos_per_mtok * d_cached, TOKENS_PER_MTOK) +
        @divTrunc(rates.output_nanos_per_mtok * d_output, TOKENS_PER_MTOK);
}

// True while `now_ms < billing.FREE_TRIAL_END_MS`. The trial ends because time
// passes — no env var, no feature flag, no database column. Private; the
// `Billing` struct's `free_trial_active` field is the public projection.

// ── Free-trial gate + rate-math (inline so tests reach the private
//    time-injected `computeStageChargeAt`) ────────────────────────────────

const POST_TRIAL_NOW_MS: i64 = billing.FREE_TRIAL_END_MS + 1_000;
const PRE_TRIAL_NOW_MS: i64 = billing.FREE_TRIAL_END_MS - 1_000;

/// The catalogue-free half of a stage charge, for the branches that price
/// without one. Null means the platform branch, which needs a connection and is
/// covered in `state/model_rate_cache_integration_test.zig` instead — a fake
/// connection here would test the fake.
fn stageChargeWithoutCatalogue(
    posture: Posture,
    elapsed_ms: i64,
    input_tokens: u32,
    cached_input_tokens: u32,
    output_tokens: u32,
    now_ms: i64,
) ?i64 {
    const rates = sliceRatesWithoutCatalogue(posture, now_ms) orelse return null;
    return sliceCharge(rates, elapsed_ms, @as(i64, input_tokens), @as(i64, cached_input_tokens), @as(i64, output_tokens));
}

test "stage charge: self_managed is the run fee only, tokens and model ignored (post-trial)" {
    // self_managed bills runFee(elapsed_ms) and nothing for tokens; it never
    // consults the catalogue, so it resolves with no connection at all.
    try std.testing.expectEqual(
        runFee(20_000),
        stageChargeWithoutCatalogue(.self_managed, 20_000, 1_000_000, 1_000_000, 1_000_000, POST_TRIAL_NOW_MS).?, // pin test: literal is the contract
    );
    // 20s of active runtime → 20 × RUN_NANOS_PER_SEC.
    try std.testing.expectEqual(
        @as(i64, 20) * RUN_NANOS_PER_SEC,
        stageChargeWithoutCatalogue(.self_managed, 20_000, 0, 0, 0, POST_TRIAL_NOW_MS).?,
    );
    // At lease issue elapsed_ms is 0 → zero run fee, zero charge.
    try std.testing.expectEqual(
        @as(i64, 0),
        stageChargeWithoutCatalogue(.self_managed, 0, 0, 0, 0, POST_TRIAL_NOW_MS).?,
    );
}

test "runFee: per-second rate with ms precision, identical for both postures" {
    // 20_000 ms = 20 s → 20 × RUN_NANOS_PER_SEC = 2_000_000 nanos.
    try std.testing.expectEqual(@as(i64, 2_000_000), runFee(20_000));
    // Sub-second precision: 1_500 ms = 1.5 s → floor(1500 × 100_000 / 1000).
    try std.testing.expectEqual(@as(i64, 150_000), runFee(1_500));
    // Zero elapsed (lease issue) → zero.
    try std.testing.expectEqual(@as(i64, 0), runFee(0));
    // The run fee does not depend on posture: self_managed with no tokens is
    // exactly the run fee for the same elapsed time.
    try std.testing.expectEqual(
        runFee(45_000),
        stageChargeWithoutCatalogue(.self_managed, 45_000, 0, 0, 0, POST_TRIAL_NOW_MS).?,
    );
}

test "stage charge: the free-trial window is zero regardless of posture / tokens / elapsed" {
    // Pre-trial → every combination short-circuits to billing.FREE_TRIAL_STAGE_NANOS.
    // PLATFORM resolving to a value here is the load-bearing part: it proves the
    // trial gate fires BEFORE the catalogue branch, which is what lets a trial
    // slice price with no connection and no catalogue row.
    try std.testing.expectEqual(
        billing.FREE_TRIAL_STAGE_NANOS,
        stageChargeWithoutCatalogue(.platform, 60_000, 800, 0, 1000, PRE_TRIAL_NOW_MS).?, // pin test: literal is the contract
    );
    try std.testing.expectEqual(
        billing.FREE_TRIAL_STAGE_NANOS,
        stageChargeWithoutCatalogue(.self_managed, 60_000, 1_000_000, 1_000_000, 1_000_000, PRE_TRIAL_NOW_MS).?, // pin test: literal is the contract
    );
    // At the cutoff (now_ms == billing.FREE_TRIAL_END_MS) the trial is over — strict
    // less-than gate; self_managed then charges the run fee.
    try std.testing.expectEqual(
        runFee(60_000),
        stageChargeWithoutCatalogue(.self_managed, 60_000, 0, 0, 0, billing.FREE_TRIAL_END_MS).?,
    );
}

test "sliceRatesWithoutCatalogue: which branches price without a catalogue read" {
    // self_managed post-trial → run rate only; token tiers stay 0 (the user's
    // own provider bills the tokens), so a metered slice is run-fee-only.
    const sm = sliceRatesWithoutCatalogue(.self_managed, POST_TRIAL_NOW_MS).?;
    try std.testing.expectEqual(RUN_NANOS_PER_SEC, sm.run_nanos_per_sec);
    try std.testing.expectEqual(@as(i64, 0), sm.input_nanos_per_mtok);
    try std.testing.expectEqual(@as(i64, 0), sm.output_nanos_per_mtok);
    // Free-trial short-circuits to all-zero before the posture switch — even
    // platform charges nothing, and issues no statement to find that out.
    const ft = sliceRatesWithoutCatalogue(.platform, PRE_TRIAL_NOW_MS).?;
    try std.testing.expectEqual(@as(i64, 0), ft.run_nanos_per_sec);
    try std.testing.expectEqual(@as(i64, 0), ft.input_nanos_per_mtok);
    // Platform, post-trial → null: the ONLY combination that needs the
    // catalogue, and therefore the only one that costs a connection. If this
    // ever returned a value, the platform branch would price from a constant.
    try std.testing.expect(sliceRatesWithoutCatalogue(.platform, POST_TRIAL_NOW_MS) == null);
}
