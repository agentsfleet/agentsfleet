//! The threshold baseline the agent sweep competes against, and the freeze
//! that keeps the comparison honest: thresholds are tuned on the calibration
//! set only, then frozen by config hash. Scoring refuses a baseline whose
//! hash drifted after calibration (see scoring.zig) — you cannot quietly
//! retune the competitor after seeing the evaluation set.

const std = @import("std");
const manifest = @import("manifest.zig");

const Sha256 = std.crypto.hash.sha2.Sha256;

pub const CONFIG_HASH_HEX_LEN = Sha256.digest_length * 2;

/// Competent multi-signal threshold rules — the "boring but honest" detector.
pub const Baseline = struct {
    error_rate_pct_max: u32,
    latency_p95_ms_max: u32,
    saturation_pct_max: u32,
    /// Consecutive breached windows required before the baseline alerts.
    multi_window_breaches_min: u32,
};

/// Recorded at calibration time; scoring verifies against it.
pub const Freeze = struct {
    tuned_on: manifest.SetKind,
    config_hash: []const u8,
};

pub const FreezeError = error{
    BaselineDrifted,
    TunedOnEvaluation,
};

/// Canonical hash over the threshold fields. Field order is fixed here — the
/// hash is over rendered key=value bytes, not struct memory, so it is stable
/// across compilers and targets.
pub fn configHashHex(b: Baseline) [CONFIG_HASH_HEX_LEN]u8 {
    // Worst case is 126 bytes (four u32 fields at maximum width); 160 leaves
    // headroom, so overflow here is a programmer bug, not a runtime input.
    var buf: [160]u8 = undefined;
    const canonical = std.fmt.bufPrint(&buf, "error_rate_pct_max={d}|latency_p95_ms_max={d}|saturation_pct_max={d}|multi_window_breaches_min={d}", .{
        b.error_rate_pct_max, b.latency_p95_ms_max, b.saturation_pct_max, b.multi_window_breaches_min,
    }) catch @panic("baseline canonical render exceeded its buffer");
    var digest: [Sha256.digest_length]u8 = undefined;
    Sha256.hash(canonical, &digest, .{});
    return std.fmt.bytesToHex(digest, .lower);
}

/// Refuses a drifted or evaluation-tuned baseline. Both failure modes are
/// refusals by construction, not warnings.
pub fn verifyFrozen(b: Baseline, f: Freeze) FreezeError!void {
    if (f.tuned_on != .calibration) return FreezeError.TunedOnEvaluation;
    const current = configHashHex(b);
    if (!std.mem.eql(u8, &current, f.config_hash)) return FreezeError.BaselineDrifted;
}

/// Would the frozen thresholds alone catch this seed? Obvious spikes above
/// the error-rate threshold are the expected threshold wins the report must
/// admit rather than hide.
pub fn thresholdCatches(b: Baseline, seed: manifest.Seed) bool {
    if (seed.clean) return false;
    if (seed.class != .obvious_spike) return false;
    return seed.magnitude_pct > b.error_rate_pct_max;
}

/// Caller must free via `.deinit()` on the returned parse.
pub fn parseBaseline(alloc: std.mem.Allocator, raw: []const u8) !std.json.Parsed(Baseline) {
    return std.json.parseFromSlice(Baseline, alloc, raw, .{ .ignore_unknown_fields = true });
}

/// Caller must free via `.deinit()` on the returned parse.
pub fn parseFreeze(alloc: std.mem.Allocator, raw: []const u8) !std.json.Parsed(Freeze) {
    return std.json.parseFromSlice(Freeze, alloc, raw, .{ .ignore_unknown_fields = true });
}
