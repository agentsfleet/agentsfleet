//! Seed manifests for the incident-response benchmark: which incidents get
//! injected into the corpus, split into calibration and evaluation halves.
//! The disjointness of that split is what keeps the benchmark honest —
//! evaluation seeds must never inform threshold tuning, and the scorer
//! refuses to run when the split is violated (see scoring.zig).

const std = @import("std");

/// Which half of the calibration/evaluation split a manifest belongs to.
pub const SetKind = enum { calibration, evaluation };

/// Incident classes the benchmark injects. The first four are code-shaped
/// (a repair proposal is a legitimate outcome); the last two must stay
/// diagnosis-only — the responder bundle is scored on NOT proposing for them.
pub const IncidentClass = enum {
    obvious_spike,
    slow_burn,
    trace_failure,
    deploy_regression,
    provider_outage,
    data_shaped,

    pub fn isCodeShaped(self: IncidentClass) bool {
        return switch (self) {
            .obvious_spike, .slow_burn, .trace_failure, .deploy_regression => true,
            .provider_outage, .data_shaped => false,
        };
    }
};

/// One injected incident — or, when `clean` is set, a quiet window the
/// detector must NOT flag (false-positive bait).
pub const Seed = struct {
    id: []const u8,
    class: IncidentClass,
    service: []const u8,
    /// Injection start, relative to the manifest epoch.
    offset_ms: i64,
    duration_ms: i64,
    /// How far above baseline the injected signal sits (error rate / latency).
    magnitude_pct: u32,
    clean: bool = false,
};

pub const SeedManifest = struct {
    set: SetKind,
    /// Fixed corpus epoch — timestamps derive from this, never the wall clock,
    /// so the same manifest always renders byte-identical telemetry.
    epoch_ms: i64,
    seeds: []const Seed,
};

pub const ValidateError = error{
    EmptySeedId,
    DuplicateSeedId,
    NonPositiveDuration,
};

pub const DisjointError = error{OverlappingSeedIds};

/// Parse a manifest from JSON bytes. Caller must free via `.deinit()` on the
/// returned `std.json.Parsed`.
pub fn parse(alloc: std.mem.Allocator, raw: []const u8) !std.json.Parsed(SeedManifest) {
    const parsed = try std.json.parseFromSlice(SeedManifest, alloc, raw, .{
        .ignore_unknown_fields = true,
    });
    errdefer parsed.deinit();
    try validate(parsed.value);
    return parsed;
}

/// Structural checks JSON decoding cannot express. Seed counts are small
/// (tens), so the duplicate scan is a plain O(n^2) comparison — no allocation.
pub fn validate(m: SeedManifest) ValidateError!void {
    for (m.seeds, 0..) |seed, i| {
        if (seed.id.len == 0) return ValidateError.EmptySeedId;
        if (seed.duration_ms <= 0) return ValidateError.NonPositiveDuration;
        for (m.seeds[i + 1 ..]) |other| {
            if (std.mem.eql(u8, seed.id, other.id)) return ValidateError.DuplicateSeedId;
        }
    }
}

/// The honesty check: no seed id may appear in both halves of the split.
pub fn assertDisjoint(a: SeedManifest, b: SeedManifest) DisjointError!void {
    for (a.seeds) |sa| {
        for (b.seeds) |sb| {
            if (std.mem.eql(u8, sa.id, sb.id)) return DisjointError.OverlappingSeedIds;
        }
    }
}
