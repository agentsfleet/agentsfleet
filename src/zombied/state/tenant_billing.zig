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

// Credit-pool cost model — M66 traction rates expressed in nanos.
// Events are free both postures; stages cost $0.001 platform / $0.0001
// self-managed. The 10× gradient between postures is the on-ramp signal:
// platform mode is convenient, self-managed is cheap to scale.
pub const Posture = tenant_provider.Mode;

/// Receive-side per-event drain. M66: zero, both postures.
pub const EVENT_NANOS: i64 = 0;

/// Stage-side platform fee, $0.001 = 1M nanos. Charged once per stage
/// execution before the executor runs under platform posture; the per-token
/// model cost (also in nanos) is added on top.
pub const STAGE_PLATFORM_NANOS: i64 = 1_000_000;

/// Stage-side self-managed fee, $0.0001 = 100K nanos. The user's own provider
/// account pays the token cost directly; we only charge the flat overhead.
pub const STAGE_SELF_MANAGED_NANOS: i64 = 100_000;

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
/// (the executor doesn't know real token counts yet). The actual cost is
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

/// Stage-side per-event charge. Under platform posture this is the flat
/// platform stage fee plus per-token cost looked up from model_rate_cache
/// (both in nanos); under self_managed it's the flat self-managed fee
/// alone (token cost lands on the user's own provider bill). Panics under
/// platform if `model` is missing from the cache — that condition should
/// have been rejected upstream by the tenant-provider PUT validator and
/// the install-skill frontmatter check.
pub fn computeStageCharge(
    provider: []const u8,
    posture: Posture,
    model: []const u8,
    input_tokens: u32,
    cached_input_tokens: u32,
    output_tokens: u32,
) i64 {
    return computeStageChargeAt(provider, posture, model, input_tokens, cached_input_tokens, output_tokens, std.time.milliTimestamp());
}

// Time-injected sibling of `computeStageCharge`. Private; inline tests below
// access it for deterministic pre/mid/post-trial coverage. Production paths
// call `computeStageCharge` (which reads the real clock). The (provider, model)
// pair keys the rate row — the same model under two providers prices apart.
fn computeStageChargeAt(
    provider: []const u8,
    posture: Posture,
    model: []const u8,
    input_tokens: u32,
    cached_input_tokens: u32,
    output_tokens: u32,
    now_ms: i64,
) i64 {
    if (isFreeTrialActive(now_ms)) return FREE_TRIAL_STAGE_NANOS;
    return switch (posture) {
        .platform => blk: {
            const rate = model_rate_cache.lookup_model_rate(provider, model) orelse
                std.debug.panic("compute_stage_charge: model '{s}' (provider '{s}') not in cached caps catalogue", .{ model, provider });
            const in_nanos = @divTrunc(rate.input_nanos_per_mtok * @as(i64, input_tokens), 1_000_000);
            const cached_nanos = @divTrunc(rate.cached_input_nanos_per_mtok * @as(i64, cached_input_tokens), 1_000_000);
            const out_nanos = @divTrunc(rate.output_nanos_per_mtok * @as(i64, output_tokens), 1_000_000);
            break :blk STAGE_PLATFORM_NANOS + in_nanos + cached_nanos + out_nanos;
        },
        .self_managed => STAGE_SELF_MANAGED_NANOS,
    };
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

test "computeStageChargeAt: self_managed returns flat overhead independent of tokens or model (post-trial)" {
    try std.testing.expectEqual(
        STAGE_SELF_MANAGED_NANOS,
        computeStageChargeAt("anthropic", .self_managed, "any-model", 0, 0, 0, POST_TRIAL_NOW_MS),
    );
    try std.testing.expectEqual(
        STAGE_SELF_MANAGED_NANOS,
        computeStageChargeAt("anthropic", .self_managed, "claude-opus-4-8", 1_000_000, 1_000_000, 1_000_000, POST_TRIAL_NOW_MS),
    );
    // self_managed never consults the rate cache, so a missing model must
    // NOT panic — only platform mode requires a cached rate.
    try std.testing.expectEqual(
        @as(i64, 100_000),
        computeStageChargeAt("anthropic", .self_managed, "model-not-in-catalogue", 100, 100, 100, POST_TRIAL_NOW_MS),
    );
}

test "computeStageChargeAt: free-trial window returns zero regardless of posture / model / tokens" {
    // Pre-trial → every combination short-circuits to FREE_TRIAL_STAGE_NANOS.
    // No rate-cache lookup happens; passing a missing model proves the
    // short-circuit fires before the platform-branch lookup.
    try std.testing.expectEqual(
        FREE_TRIAL_STAGE_NANOS,
        computeStageChargeAt("pioneer", .platform, "model-not-in-catalogue", 800, 0, 1000, PRE_TRIAL_NOW_MS),
    );
    try std.testing.expectEqual(
        FREE_TRIAL_STAGE_NANOS,
        computeStageChargeAt("anthropic", .self_managed, "any-model", 1_000_000, 1_000_000, 1_000_000, PRE_TRIAL_NOW_MS),
    );
    // At the cutoff (now_ms == FREE_TRIAL_END_MS) the trial is over —
    // strict less-than gate.
    try std.testing.expectEqual(
        @as(i64, 100_000),
        computeStageChargeAt("anthropic", .self_managed, "any-model", 0, 0, 0, FREE_TRIAL_END_MS),
    );
}

test "isFreeTrialActive: strict-less-than gate on FREE_TRIAL_END_MS" {
    try std.testing.expect(isFreeTrialActive(0));
    try std.testing.expect(isFreeTrialActive(FREE_TRIAL_END_MS - 1));
    try std.testing.expect(!isFreeTrialActive(FREE_TRIAL_END_MS));
    try std.testing.expect(!isFreeTrialActive(FREE_TRIAL_END_MS + 1_000_000));
}

test {
    _ = @import("tenant_billing_test.zig");
}
