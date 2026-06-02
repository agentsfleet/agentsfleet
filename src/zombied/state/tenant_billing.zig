const std = @import("std");
const pg = @import("pg");
const store = @import("tenant_billing_store.zig");
const tenant_provider = @import("tenant_provider.zig");
const model_rate_cache = @import("model_rate_cache.zig");
const logging = @import("log");

const log = logging.scoped(.state);

/// Canonical nanos-per-USD conversion factor. 1 USD = 1_000_000_000 nanos
/// (1 nano = 1/1,000,000,000 USD). Mirrors `NANOS_PER_USD` in
/// `ui/packages/app/lib/types.ts` and `zombiectl/src/constants/billing.js`.
pub const NANOS_PER_USD: i64 = 1_000_000_000;

/// $5 starter grant in nanos.
pub const STARTER_CREDIT_NANOS: i64 = 5 * NANOS_PER_USD;
const BOOTSTRAP_GRANT_SOURCE = "bootstrap_starter_grant";

// Credit-pool cost model — expressed in nanos. Event receipts are free under
// both postures. Active agent runtime is metered per second at the single
// RUN_NANOS_PER_SEC rate (identical for both postures); platform posture adds
// the per-token model cost on top, self-managed leaves tokens to the user's
// own provider bill.
pub const Posture = tenant_provider.Mode;

/// Receive-side per-event drain. Zero under both postures.
pub const EVENT_NANOS: i64 = 0;

/// Run-time rate: $0.0001/sec = 100K nanos per active second (≈ $0.36/hr),
/// charged identically under both postures. Runtime is metered by the second
/// as the agent works — not estimated once at lease issue — so a slice's run
/// fee is `runFee(elapsed_ms)`. The per-token model cost (platform posture
/// only) is added on top.
pub const RUN_NANOS_PER_SEC: i64 = 100_000;

/// Promotional free-trial cutoff (UTC). `2026-08-01T00:00:00Z` — first
/// instant after July 31, 2026. While `now_ms < FREE_TRIAL_END_MS`,
/// computeStageCharge returns `FREE_TRIAL_STAGE_NANOS` regardless of
/// posture / model / token count; after, it falls through to the standard
/// rate constants. The numeric value mirrors the JS/TS twins in
/// `ui/packages/website/src/lib/rates.ts`, `ui/packages/app/lib/types.ts`,
/// and `zombiectl/src/constants/billing.js` (cross-tier parity rule, value
/// match — these are private to keep new pub surface at zero; the HTTP
/// handler reads them through the `Billing` struct's `free_trial_*` fields
/// rather than importing the const directly). Customer surface for live
/// rates and active windows: usezombie.com/#pricing.
const FREE_TRIAL_END_MS: i64 = 1_785_542_400_000;
const FREE_TRIAL_STAGE_NANOS: i64 = 0;

/// Conservative estimate floors used by the gate-time stage-cost projection
/// (the runner doesn't know real token counts yet). The actual cost is
/// charged at this floor; v3 may add reconciliation against StageResult.
pub const ESTIMATE_FLOOR_INPUT_TOKENS: u32 = 100;
pub const ESTIMATE_FLOOR_OUTPUT_TOKENS: u32 = 100;

pub const Billing = struct {
    balance_nanos: i64,
    grant_source: []const u8,
    updated_at_ms: i64,
    exhausted_at_ms: ?i64,
    free_trial_active: bool,
    free_trial_ends_at_ms: i64,
};

pub const DebitResult = struct { balance_nanos: i64, updated_at_ms: i64 };

pub fn provision(
    conn: *pg.Conn,
    tenant_id: []const u8,
    balance_nanos: i64,
    grant_source: []const u8,
) !void {
    try store.insertIfAbsent(conn, tenant_id, balance_nanos, grant_source);
    log.info("tenant_billing_provisioned", .{ .tenant_id = tenant_id, .balance_nanos = balance_nanos, .source = grant_source });
}

/// Insert the one-time $5 starter grant for a new tenant. Called from the
/// tenant-create transaction in signup_bootstrap. Idempotent via the
/// underlying ON CONFLICT DO NOTHING.
pub fn insertStarterGrant(conn: *pg.Conn, tenant_id: []const u8) !void {
    return provision(conn, tenant_id, STARTER_CREDIT_NANOS, BOOTSTRAP_GRANT_SOURCE);
}

/// Receive-side per-event charge. M66: zero both postures.
pub fn computeReceiveCharge(posture: Posture) i64 {
    _ = posture;
    return EVENT_NANOS;
}

/// Run-time fee for `elapsed_ms` of active agent runtime, charged identically
/// under both postures. `RUN_NANOS_PER_SEC` is per-second; the ms→s division
/// uses the same `@divTrunc` discipline as the per-mtok token math. i64-safe:
/// `elapsed_ms` is bounded by the lease's MAX_RUNTIME_MS, so `elapsed_ms *
/// RUN_NANOS_PER_SEC` stays well inside i64. At lease issue `elapsed_ms` is 0,
/// so the run fee is 0 and only the token estimate (platform) is charged.
fn runFee(elapsed_ms: i64) i64 {
    return @divTrunc(elapsed_ms * RUN_NANOS_PER_SEC, 1000);
}

/// Per-slice stage charge: a run fee for `elapsed_ms` of active runtime plus,
/// under platform posture, the per-token model cost of the token counts looked
/// up from model_rate_cache (all in nanos). Under self_managed the run fee is
/// the whole charge — token cost lands on the user's own provider bill. Panics
/// under platform if `model` is missing from the cache — that condition should
/// have been rejected upstream by the tenant-provider PUT validator and the
/// install-skill frontmatter check.
pub fn computeStageCharge(
    provider: []const u8,
    posture: Posture,
    model: []const u8,
    elapsed_ms: i64,
    input_tokens: u32,
    cached_input_tokens: u32,
    output_tokens: u32,
) i64 {
    return computeStageChargeAt(provider, posture, model, elapsed_ms, input_tokens, cached_input_tokens, output_tokens, std.time.milliTimestamp());
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

/// Resolve the slice rates with the SAME branching `computeStageChargeAt` uses:
/// free-trial (`now_ms < FREE_TRIAL_END_MS`) → all zero (a metered run charges
/// 0); self_managed → run rate only (token tiers 0, recorded-not-charged);
/// platform → run rate + the model's three token tiers. A platform model absent
/// from the rate cache returns `null` — the renew handler meters run-fee-only +
/// logs (the live path never panics; a lease could not have been issued for an
/// uncatalogued platform model, so this is a cache-eviction edge). The
/// (provider, model) pair keys the rate row — same model, two providers, two rates.
pub fn resolveRenewSliceRates(provider: []const u8, posture: Posture, model: []const u8, now_ms: i64) ?SliceRates {
    if (isFreeTrialActive(now_ms)) return SliceRates{ .run_nanos_per_sec = 0, .input_nanos_per_mtok = 0, .cached_input_nanos_per_mtok = 0, .output_nanos_per_mtok = 0 };
    return switch (posture) {
        .self_managed => SliceRates{ .run_nanos_per_sec = RUN_NANOS_PER_SEC, .input_nanos_per_mtok = 0, .cached_input_nanos_per_mtok = 0, .output_nanos_per_mtok = 0 },
        .platform => blk: {
            const rate = model_rate_cache.lookup_model_rate(provider, model) orelse break :blk null;
            break :blk SliceRates{ .run_nanos_per_sec = RUN_NANOS_PER_SEC, .input_nanos_per_mtok = rate.input_nanos_per_mtok, .cached_input_nanos_per_mtok = rate.cached_input_nanos_per_mtok, .output_nanos_per_mtok = rate.output_nanos_per_mtok };
        },
    };
}

/// Apply slice rates to a set of deltas — the exact arithmetic the renewal CTE
/// reproduces in SQL (per-tier `@divTrunc(rate*Δ, 1e6)` + ms→s `@divTrunc(Δt*run,
/// 1000)`; Postgres bigint `/` truncates toward zero, matching for Δ≥0). This is
/// the reference the SQL==Zig pin test asserts against.
pub fn sliceCharge(rates: SliceRates, elapsed_ms: i64, d_input: i64, d_cached: i64, d_output: i64) i64 {
    return @divTrunc(elapsed_ms * rates.run_nanos_per_sec, 1000) +
        @divTrunc(rates.input_nanos_per_mtok * d_input, 1_000_000) +
        @divTrunc(rates.cached_input_nanos_per_mtok * d_cached, 1_000_000) +
        @divTrunc(rates.output_nanos_per_mtok * d_output, 1_000_000);
}

// Time-injected sibling of `computeStageCharge`. Private; inline tests below
// access it for deterministic pre/mid/post-trial coverage. Production paths
// call `computeStageCharge` (which reads the real clock). Delegates rate
// resolution + arithmetic to the shared slice helpers so the per-stage estimate
// and the per-renewal Δ-charge can never diverge.
fn computeStageChargeAt(
    provider: []const u8,
    posture: Posture,
    model: []const u8,
    elapsed_ms: i64,
    input_tokens: u32,
    cached_input_tokens: u32,
    output_tokens: u32,
    now_ms: i64,
) i64 {
    const rates = resolveRenewSliceRates(provider, posture, model, now_ms) orelse
        std.debug.panic("compute_stage_charge: model '{s}' (provider '{s}') not in cached caps catalogue", .{ model, provider });
    return sliceCharge(rates, elapsed_ms, @as(i64, input_tokens), @as(i64, cached_input_tokens), @as(i64, output_tokens));
}

// True while `now_ms < FREE_TRIAL_END_MS`. The trial ends because time
// passes — no env var, no feature flag, no database column. Private; the
// `Billing` struct's `free_trial_active` field is the public projection.
fn isFreeTrialActive(now_ms: i64) bool {
    return now_ms < FREE_TRIAL_END_MS;
}

pub fn debit(conn: *pg.Conn, tenant_id: []const u8, nanos: i64) !DebitResult {
    const r = try store.debit(conn, tenant_id, nanos);
    return .{ .balance_nanos = r.balance_nanos, .updated_at_ms = r.updated_at_ms };
}

/// Atomically stamp `balance_exhausted_at` on the first CreditExhausted debit.
/// Returns true if this call transitioned the row (first exhaust), false if
/// the row was already marked. Callers use the return value to gate the
/// one-shot `balance_exhausted_first_debit` activity event.
pub fn markExhausted(conn: *pg.Conn, tenant_id: []const u8) !bool {
    return store.markExhausted(conn, tenant_id);
}

/// Clear `balance_exhausted_at` when a tenant is replenished outside the
/// regular `debit` path (admin manual credit, Stripe top-up when wired,
/// etc.). `debit` already clears on a successful deduction, so callers
/// that debit after top-up do not need this — but paths that add credit
/// without a matching debit (refunds, grants, admin SQL) MUST call it
/// or the `stop` gate stays permanently closed.
pub fn clearExhausted(conn: *pg.Conn, tenant_id: []const u8) !bool {
    return store.clearExhausted(conn, tenant_id);
}

/// Caller owns the grant_source slice. `free_trial_active` reflects the
/// promotional window at call time (clock-derived); `free_trial_ends_at_ms`
/// is the cutoff constant. Surface for both `GET /v1/tenants/me/billing`
/// and `zombiectl doctor --json`.
pub fn getBilling(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    tenant_id: []const u8,
) !?Billing {
    const row = (try store.loadByTenant(conn, alloc, tenant_id)) orelse return null;
    const now_ms = std.time.milliTimestamp();
    return .{
        .balance_nanos = row.balance_nanos,
        .grant_source = row.grant_source,
        .updated_at_ms = row.updated_at_ms,
        .exhausted_at_ms = row.exhausted_at_ms,
        .free_trial_active = isFreeTrialActive(now_ms),
        .free_trial_ends_at_ms = FREE_TRIAL_END_MS,
    };
}

pub fn resolveTenantFromWorkspace(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) ![]u8 {
    return store.resolveTenantFromWorkspace(conn, alloc, workspace_id);
}

// ── Free-trial gate + rate-math (inline so tests reach the private
//    time-injected `computeStageChargeAt`) ────────────────────────────────

const POST_TRIAL_NOW_MS: i64 = FREE_TRIAL_END_MS + 1_000;
const PRE_TRIAL_NOW_MS: i64 = FREE_TRIAL_END_MS - 1_000;

test "computeStageChargeAt: self_managed charge is the run fee only, tokens and model ignored (post-trial)" {
    // self_managed bills runFee(elapsed_ms) and nothing for tokens; it never
    // consults the rate cache, so a missing model must NOT panic.
    try std.testing.expectEqual(
        runFee(20_000),
        computeStageChargeAt("anthropic", .self_managed, "model-not-in-catalogue", 20_000, 1_000_000, 1_000_000, 1_000_000, POST_TRIAL_NOW_MS),
    );
    // 20s of active runtime → 20 × RUN_NANOS_PER_SEC.
    try std.testing.expectEqual(
        @as(i64, 20) * RUN_NANOS_PER_SEC,
        computeStageChargeAt("anthropic", .self_managed, "any-model", 20_000, 0, 0, 0, POST_TRIAL_NOW_MS),
    );
    // At lease issue elapsed_ms is 0 → zero run fee, zero charge.
    try std.testing.expectEqual(
        @as(i64, 0),
        computeStageChargeAt("anthropic", .self_managed, "any-model", 0, 0, 0, 0, POST_TRIAL_NOW_MS),
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
        computeStageChargeAt("anthropic", .self_managed, "any-model", 45_000, 0, 0, 0, POST_TRIAL_NOW_MS),
    );
}

test "computeStageChargeAt: free-trial window returns zero regardless of posture / model / tokens / elapsed" {
    // Pre-trial → every combination short-circuits to FREE_TRIAL_STAGE_NANOS.
    // No rate-cache lookup happens; passing a missing model proves the
    // short-circuit fires before the platform-branch lookup.
    try std.testing.expectEqual(
        FREE_TRIAL_STAGE_NANOS,
        computeStageChargeAt("pioneer", .platform, "model-not-in-catalogue", 60_000, 800, 0, 1000, PRE_TRIAL_NOW_MS),
    );
    try std.testing.expectEqual(
        FREE_TRIAL_STAGE_NANOS,
        computeStageChargeAt("anthropic", .self_managed, "any-model", 60_000, 1_000_000, 1_000_000, 1_000_000, PRE_TRIAL_NOW_MS),
    );
    // At the cutoff (now_ms == FREE_TRIAL_END_MS) the trial is over — strict
    // less-than gate; self_managed then charges the run fee.
    try std.testing.expectEqual(
        runFee(60_000),
        computeStageChargeAt("anthropic", .self_managed, "any-model", 60_000, 0, 0, 0, FREE_TRIAL_END_MS),
    );
}

test "isFreeTrialActive: strict-less-than gate on FREE_TRIAL_END_MS" {
    try std.testing.expect(isFreeTrialActive(0));
    try std.testing.expect(isFreeTrialActive(FREE_TRIAL_END_MS - 1));
    try std.testing.expect(!isFreeTrialActive(FREE_TRIAL_END_MS));
    try std.testing.expect(!isFreeTrialActive(FREE_TRIAL_END_MS + 1_000_000));
}

test "resolveRenewSliceRates: posture/trial branches, and platform cache-miss yields null (never panics)" {
    // self_managed post-trial → run rate only; token tiers stay 0 (the user's
    // own provider bills the tokens), so a metered slice is run-fee-only.
    const sm = resolveRenewSliceRates("self-managed-test", .self_managed, "any-model", POST_TRIAL_NOW_MS).?;
    try std.testing.expectEqual(RUN_NANOS_PER_SEC, sm.run_nanos_per_sec);
    try std.testing.expectEqual(@as(i64, 0), sm.input_nanos_per_mtok);
    try std.testing.expectEqual(@as(i64, 0), sm.output_nanos_per_mtok);
    // Free-trial short-circuits to all-zero before any cache lookup — even
    // platform with a real model id charges nothing.
    const ft = resolveRenewSliceRates("anthropic", .platform, "claude-sonnet-4-6", PRE_TRIAL_NOW_MS).?;
    try std.testing.expectEqual(@as(i64, 0), ft.run_nanos_per_sec);
    try std.testing.expectEqual(@as(i64, 0), ft.input_nanos_per_mtok);
    // Platform post-trial, model absent from the process rate cache → null. The
    // renew/settle caller meters run-fee-only on null and NEVER panics the live
    // path — unlike computeStageChargeAt, whose issue-time estimate panics.
    try std.testing.expect(resolveRenewSliceRates("anthropic", .platform, "model-not-in-cache-zzz", POST_TRIAL_NOW_MS) == null);
}

test {
    _ = @import("tenant_billing_test.zig");
}
