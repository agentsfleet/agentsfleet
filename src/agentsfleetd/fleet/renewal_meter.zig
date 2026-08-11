//! The metering inputs a renew or settle statement binds: the runner's
//! cumulative token counts plus the four resolved per-unit slice rates.
//!
//! Split from `renewal.zig` at the resolve/execute seam (RULE FLL): that
//! module owns the fenced statement; this one owns turning a runner's report
//! into the rates the statement charges at. Shared by `service_renew` and
//! `service_report` so both meter identically.

const pg = @import("pg");
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const billing_rates = @import("../state/tenant_billing_rates.zig");
const billing_store = @import("../state/tenant_billing_store.zig");
const tenant_provider = @import("../state/tenant_provider.zig");

const log = logging.scoped(.fleet_metering);

/// The runner's cumulative token counts + the resolved per-unit slice rates for
/// this renewal. Cumulatives are diffed against the lease's metering cursor IN
/// the CTE (this struct never carries deltas — no double-count). Rates already
/// encode posture + free-trial (all-zero during the trial; token tiers zero
/// under self_managed), so the SQL applies them uniformly.
pub const MeterInputs = struct {
    cumulative_input: i64 = 0,
    cumulative_cached: i64 = 0,
    cumulative_output: i64 = 0,
    run_nanos_per_sec: i64 = 0,
    input_nanos_per_mtok: i64 = 0,
    cached_input_nanos_per_mtok: i64 = 0,
    output_nanos_per_mtok: i64 = 0,
};

/// Resolve the four slice rates (free-trial / posture aware) and pair them with
/// the runner's cumulative token counts.
///
/// Takes the caller's already-acquired connection: the platform branch prices
/// against the catalogue generation that connection observes, so a slice
/// can never be metered at a rate the catalogue has moved past. Free-trial and
/// self-managed slices issue no statement — they never reach the catalogue.
///
/// Never panics and never propagates: both a generation that cannot be verified
/// and a model the catalogue does not carry meter run-fee-only and log. See the
/// body for why those two are logged apart.
pub fn buildMeterInputs(
    conn: *pg.Conn,
    tenant_id: []const u8,
    provider: []const u8,
    posture: tenant_provider.Mode,
    model: []const u8,
    now_ms: i64,
    cum_input: u32,
    cum_cached: u32,
    cum_output: u32,
) MeterInputs {
    // Two distinct failures, deliberately kept apart. An ERROR means the
    // catalogue generation could not be established, so no rate here is known to
    // be current — metering the token tiers from anything would be pricing a
    // slice against an unverified generation, which is the one thing that must
    // never happen. A NULL means the catalogue authoritatively has no such
    // row. Both land on run-fee-only, but they are logged apart because one is a
    // database fault to page on and the other is a catalogue gap to fix.
    //
    // Run-fee-only is the fail-closed answer HERE, not a fallback: the token
    // component is dropped rather than guessed, and the run keeps going. The
    // alternative — refusing the renewal — kills a live agent mid-run over a
    // transient database fault, which is the posture `budgetRefusal` already
    // rejected for exactly this trade.
    // The tenant's own trial boundary. A lookup failure prices as open-ended
    // rather than refusing the renewal: the same posture the two branches below
    // take, and the cheaper error for a live agent mid-run.
    const trial_ends_at_ms: ?i64 = billing_store.loadTrialBoundary(conn, tenant_id) catch null;
    const resolved: ?billing_rates.SliceRates =
        billing_rates.resolveRenewSliceRates(conn, provider, posture, model, trial_ends_at_ms, now_ms) catch |err| unverified: {
            log.warn("meter_rate_generation_unverified_run_fee_only", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .provider = provider, .model = model, .err = @errorName(err) });
            break :unverified null;
        };
    const rates = resolved orelse blk: {
        log.warn("meter_rate_missing_run_fee_only", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .provider = provider, .model = model });
        break :blk billing_rates.SliceRates{ .run_nanos_per_sec = tenant_billing.RUN_NANOS_PER_SEC, .input_nanos_per_mtok = 0, .cached_input_nanos_per_mtok = 0, .output_nanos_per_mtok = 0 };
    };
    return .{
        .cumulative_input = @intCast(cum_input),
        .cumulative_cached = @intCast(cum_cached),
        .cumulative_output = @intCast(cum_output),
        .run_nanos_per_sec = rates.run_nanos_per_sec,
        .input_nanos_per_mtok = rates.input_nanos_per_mtok,
        .cached_input_nanos_per_mtok = rates.cached_input_nanos_per_mtok,
        .output_nanos_per_mtok = rates.output_nanos_per_mtok,
    };
}
