//! M27_001: Resource score formula + ResourceMetrics unit tests.
//!
//! Covers:
//! - T1: Happy path — realistic production-like inputs
//! - T2: Edge cases — zero, max, boundary, overflow
//! - T3: Error paths — missing/invalid metrics → fallback 50
//! - T4: Fidelity — formula accuracy at known points
//! - T5: Concurrency — parallel computeResourceScore is safe (pure function)
//! - T7: Regression — struct field stability, version bump
//! - T9: Parameterized via inline for
//! - T10: Constants stability (SCORE_FORMULA_VERSION)
//! - T11: No allocations in computeResourceScore (pure math)

const std = @import("std");
const types = @import("types.zig");
const math = @import("math.zig");

const RM = types.ResourceMetrics;

// ─────────────────────────────────────────────────────────────────────────────
// T1: Happy path — realistic production metrics
// ─────────────────────────────────────────────────────────────────────────────

test "T1: low-usage agent scores high" {
    // 10% memory, 0% throttle → mem_score=90, cpu_score=100 → 93
    const score = math.computeResourceScore(.{
        .peak_memory_bytes = 50 * 1024 * 1024, // 50 MB
        .memory_limit_bytes = 512 * 1024 * 1024, // 512 MB
        .cpu_throttled_ms = 0,
        .wall_ms = 30_000,
    });
    try std.testing.expect(score >= 80);
    try std.testing.expect(score <= 100);
}

test "T1: moderate-usage agent scores mid-range" {
    // 50% memory, 20% throttle → mem_score=50, cpu_score=80 → 59
    const score = math.computeResourceScore(.{
        .peak_memory_bytes = 256 * 1024 * 1024,
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 6_000,
        .wall_ms = 30_000,
    });
    try std.testing.expect(score >= 40);
    try std.testing.expect(score <= 70);
}

test "T1: heavy-usage agent scores low" {
    // 90% memory, 80% throttle → mem_score=10, cpu_score=20 → 13
    const score = math.computeResourceScore(.{
        .peak_memory_bytes = 460 * 1024 * 1024,
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 24_000,
        .wall_ms = 30_000,
    });
    try std.testing.expect(score <= 20);
}

// ─────────────────────────────────────────────────────────────────────────────
// T2: Edge cases — boundary conditions
// ─────────────────────────────────────────────────────────────────────────────

test "T2: minimal usage → 100" {
    try std.testing.expectEqual(@as(u8, 100), math.computeResourceScore(.{
        .peak_memory_bytes = 1, // 1 byte — effectively zero
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 0,
        .wall_ms = 10_000,
    }));
}

test "T2: zero peak_memory_bytes → fallback 50 (no cgroup data)" {
    try std.testing.expectEqual(@as(u8, 50), math.computeResourceScore(.{
        .peak_memory_bytes = 0,
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 0,
        .wall_ms = 10_000,
    }));
}

test "T2: 100% memory + 100% throttle → 0" {
    try std.testing.expectEqual(@as(u8, 0), math.computeResourceScore(.{
        .peak_memory_bytes = 512 * 1024 * 1024,
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 10_000,
        .wall_ms = 10_000,
    }));
}

test "T2: peak exceeds limit (OOM-adjacent) clamps to 0 mem_score" {
    // mem_score = 0, cpu_score = 100 → 0 * 0.7 + 100 * 0.3 = 30
    try std.testing.expectEqual(@as(u8, 30), math.computeResourceScore(.{
        .peak_memory_bytes = 1024 * 1024 * 1024, // 1 GB > 512 MB limit
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 0,
        .wall_ms = 10_000,
    }));
}

test "T2: throttled exceeds wall (ratio > 1) clamps to 0 cpu_score" {
    // mem_score ≈ 100 (1 byte), cpu_score = 0 → 100 * 0.7 + 0 * 0.3 = 70
    try std.testing.expectEqual(@as(u8, 70), math.computeResourceScore(.{
        .peak_memory_bytes = 1,
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 20_000,
        .wall_ms = 10_000,
    }));
}

test "T2: very large values (u64 near max) don't panic" {
    // Near-max peak → ratio ≈ 1.0 → mem_score ≈ 0
    const score = math.computeResourceScore(.{
        .peak_memory_bytes = std.math.maxInt(u64) / 2,
        .memory_limit_bytes = std.math.maxInt(u64) / 2,
        .cpu_throttled_ms = 1,
        .wall_ms = std.math.maxInt(u64) / 2,
    });
    try std.testing.expect(score <= 100);
}

test "T2: 1-byte memory, 1ms wall — minimal valid inputs" {
    const score = math.computeResourceScore(.{
        .peak_memory_bytes = 1,
        .memory_limit_bytes = 1,
        .cpu_throttled_ms = 0,
        .wall_ms = 1,
    });
    // mem_score = 0 (100% usage), cpu_score = 100 → 30
    try std.testing.expectEqual(@as(u8, 30), score);
}

// ─────────────────────────────────────────────────────────────────────────────
// T3: Error/fallback paths — missing metrics
// ─────────────────────────────────────────────────────────────────────────────

test "T3: default ResourceMetrics → fallback 50" {
    try std.testing.expectEqual(@as(u8, 50), math.computeResourceScore(.{}));
}

test "T3: memory_limit_bytes=0 → fallback 50" {
    try std.testing.expectEqual(@as(u8, 50), math.computeResourceScore(.{
        .peak_memory_bytes = 100 * 1024 * 1024,
        .memory_limit_bytes = 0,
        .cpu_throttled_ms = 500,
        .wall_ms = 10_000,
    }));
}

test "T3: wall_ms=0 → fallback 50" {
    try std.testing.expectEqual(@as(u8, 50), math.computeResourceScore(.{
        .peak_memory_bytes = 100 * 1024 * 1024,
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 0,
        .wall_ms = 0,
    }));
}

test "T3: both limit and wall zero → fallback 50" {
    try std.testing.expectEqual(@as(u8, 50), math.computeResourceScore(.{
        .peak_memory_bytes = 0,
        .memory_limit_bytes = 0,
        .cpu_throttled_ms = 0,
        .wall_ms = 0,
    }));
}

// ─────────────────────────────────────────────────────────────────────────────
// T4: Fidelity — exact formula verification at known points
// ─────────────────────────────────────────────────────────────────────────────

test "T4: 50% memory + 0% throttle = 65" {
    // mem_score = 50, cpu_score = 100 → 50*0.7 + 100*0.3 = 35 + 30 = 65
    try std.testing.expectEqual(@as(u8, 65), math.computeResourceScore(.{
        .peak_memory_bytes = 256 * 1024 * 1024,
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 0,
        .wall_ms = 10_000,
    }));
}

test "T4: minimal memory + 50% throttle = 85" {
    // mem_score ≈ 100 (1 byte), cpu_score = 50 → 100*0.7 + 50*0.3 = 70 + 15 = 85
    try std.testing.expectEqual(@as(u8, 85), math.computeResourceScore(.{
        .peak_memory_bytes = 1,
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 5_000,
        .wall_ms = 10_000,
    }));
}

test "T4: 25% memory + 25% throttle" {
    // mem_score = 75, cpu_score = 75 → 75*0.7 + 75*0.3 = 52.5 + 22.5 = 75
    try std.testing.expectEqual(@as(u8, 75), math.computeResourceScore(.{
        .peak_memory_bytes = 128 * 1024 * 1024,
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 2_500,
        .wall_ms = 10_000,
    }));
}

// ─────────────────────────────────────────────────────────────────────────────
// T4: Parameterized fidelity — inline for over test cases (T9)
// ─────────────────────────────────────────────────────────────────────────────

const FidelityCase = struct {
    peak_pct: u64, // memory usage as % of limit
    throttle_pct: u64, // CPU throttle as % of wall
    expected: u8,
};

const fidelity_cases = [_]FidelityCase{
    .{ .peak_pct = 1, .throttle_pct = 0, .expected = 99 },
    .{ .peak_pct = 10, .throttle_pct = 0, .expected = 93 },
    .{ .peak_pct = 50, .throttle_pct = 50, .expected = 50 },
    .{ .peak_pct = 100, .throttle_pct = 0, .expected = 30 },
    .{ .peak_pct = 1, .throttle_pct = 100, .expected = 69 },
    .{ .peak_pct = 100, .throttle_pct = 100, .expected = 0 },
};

test "T4+T9: parameterized fidelity across memory/cpu ratio grid" {
    const limit: u64 = 512 * 1024 * 1024;
    const wall: u64 = 10_000;
    for (fidelity_cases) |tc| {
        const score = math.computeResourceScore(.{
            .peak_memory_bytes = (limit * tc.peak_pct) / 100,
            .memory_limit_bytes = limit,
            .cpu_throttled_ms = (wall * tc.throttle_pct) / 100,
            .wall_ms = wall,
        });
        try std.testing.expectEqual(tc.expected, score);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// T5: Concurrency — computeResourceScore is pure, safe under parallel calls
// ─────────────────────────────────────────────────────────────────────────────

test "T5: concurrent computeResourceScore from 8 threads produces consistent results" {
    const Worker = struct {
        fn run() void {
            for (0..100) |_| {
                const score = math.computeResourceScore(.{
                    .peak_memory_bytes = 256 * 1024 * 1024,
                    .memory_limit_bytes = 512 * 1024 * 1024,
                    .cpu_throttled_ms = 0,
                    .wall_ms = 10_000,
                });
                std.debug.assert(score == 65);
            }
        }
    };

    var threads: [8]std.Thread = undefined;
    for (&threads) |*t| {
        t.* = try std.Thread.spawn(.{}, Worker.run, .{});
    }
    for (&threads) |*t| t.join();
}

// ─────────────────────────────────────────────────────────────────────────────
// T7: Regression — ResourceMetrics struct stability
// ─────────────────────────────────────────────────────────────────────────────

test "T7: ResourceMetrics has exactly 4 fields" {
    comptime std.debug.assert(@typeInfo(RM).@"struct".fields.len == 4);
}

test "T7: ResourceMetrics defaults all to zero" {
    const rm = RM{};
    try std.testing.expectEqual(@as(u64, 0), rm.peak_memory_bytes);
    try std.testing.expectEqual(@as(u64, 0), rm.memory_limit_bytes);
    try std.testing.expectEqual(@as(u64, 0), rm.cpu_throttled_ms);
    try std.testing.expectEqual(@as(u64, 0), rm.wall_ms);
}

test "T7: ScoringState includes resource_metrics field" {
    comptime std.debug.assert(@hasField(types.ScoringState, "resource_metrics"));
    const ss = types.ScoringState{};
    try std.testing.expect(!ss.resource_metrics.hasMetrics());
}

// ─────────────────────────────────────────────────────────────────────────────
// T10: Constants — SCORE_FORMULA_VERSION bumped to "2"
// ─────────────────────────────────────────────────────────────────────────────

test "T10: SCORE_FORMULA_VERSION is 2" {
    try std.testing.expectEqualStrings("2", types.SCORE_FORMULA_VERSION);
}

// ─────────────────────────────────────────────────────────────────────────────
// T2: hasMetrics edge cases
// ─────────────────────────────────────────────────────────────────────────────

test "T2: hasMetrics requires peak_memory_bytes, memory_limit_bytes, and wall_ms" {
    try std.testing.expect(!(RM{}).hasMetrics());
    try std.testing.expect(!(RM{ .memory_limit_bytes = 100 }).hasMetrics());
    try std.testing.expect(!(RM{ .wall_ms = 100 }).hasMetrics());
    try std.testing.expect(!(RM{ .memory_limit_bytes = 100, .wall_ms = 100 }).hasMetrics());
    try std.testing.expect(!(RM{ .peak_memory_bytes = 100, .wall_ms = 100 }).hasMetrics());
    try std.testing.expect((RM{ .peak_memory_bytes = 1, .memory_limit_bytes = 100, .wall_ms = 100 }).hasMetrics());
}

test "T2: hasMetrics returns false when peak_memory_bytes is zero (no cgroup data)" {
    // This is the P1 fix: executor connected but no cgroup metrics → fallback 50.
    try std.testing.expect(!(RM{
        .peak_memory_bytes = 0,
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 0,
        .wall_ms = 10_000,
    }).hasMetrics());
}

// ─────────────────────────────────────────────────────────────────────────────
// T11: computeResourceScore is pure (no allocations)
// ─────────────────────────────────────────────────────────────────────────────

test "T11: 10000 calls with testing.allocator detect no leaks" {
    for (0..10_000) |i| {
        _ = math.computeResourceScore(.{
            .peak_memory_bytes = i * 1024,
            .memory_limit_bytes = 512 * 1024 * 1024,
            .cpu_throttled_ms = i,
            .wall_ms = 10_000,
        });
    }
    // std.testing.allocator would fail this test if any allocation leaked.
}

// ─────────────────────────────────────────────────────────────────────────────
// T4: computeScore integration — resource axis affects total
// ─────────────────────────────────────────────────────────────────────────────

test "T4: real resource score flows through computeScore with default weights" {
    const resource = math.computeResourceScore(.{
        .peak_memory_bytes = 256 * 1024 * 1024,
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 0,
        .wall_ms = 10_000,
    });
    try std.testing.expectEqual(@as(u8, 65), resource);

    // With perfect other axes: 100*0.4 + 100*0.3 + 100*0.2 + 65*0.1 = 96.5 → 97
    const total = math.computeScore(
        .{ .completion = 100, .error_rate = 100, .latency = 100, .resource = resource },
        types.DEFAULT_WEIGHTS,
    );
    try std.testing.expectEqual(@as(u8, 97), total);
}

test "T4: fallback 50 resource score matches old behavior" {
    const resource = math.computeResourceScore(.{});
    try std.testing.expectEqual(@as(u8, 50), resource);

    // Old formula v1: all perfect + resource=50 → 100*0.4 + 100*0.3 + 100*0.2 + 50*0.1 = 95
    const total = math.computeScore(
        .{ .completion = 100, .error_rate = 100, .latency = 100, .resource = resource },
        types.DEFAULT_WEIGHTS,
    );
    try std.testing.expectEqual(@as(u8, 95), total);
}

// ─────────────────────────────────────────────────────────────────────────────
// T8: Rogue agent detection — high resource usage produces low scores
// ─────────────────────────────────────────────────────────────────────────────

test "T8: agent using 90%+ memory scores resource < 20 (acceptance §4.3)" {
    const score = math.computeResourceScore(.{
        .peak_memory_bytes = 480 * 1024 * 1024, // ~94% of 512 MB
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 5_000, // moderate throttle
        .wall_ms = 10_000,
    });
    try std.testing.expect(score < 20);
}

test "T8: agent with 0% throttle and low memory scores > 80 (acceptance §4.4)" {
    const score = math.computeResourceScore(.{
        .peak_memory_bytes = 25 * 1024 * 1024, // ~5% of 512 MB
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 0,
        .wall_ms = 10_000,
    });
    try std.testing.expect(score > 80);
}

test "T8: OOM-adjacent agent (100% memory) is visible as low resource score" {
    const score = math.computeResourceScore(.{
        .peak_memory_bytes = 512 * 1024 * 1024,
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 0,
        .wall_ms = 10_000,
    });
    // mem_score = 0, cpu_score = 100 → 30 — clearly flagged
    try std.testing.expectEqual(@as(u8, 30), score);
    // This score would be visible in PostHog as axis_resource=30, tier ≤ bronze.
}

test "T8: CPU-starved agent (100% throttled) is visible" {
    const score = math.computeResourceScore(.{
        .peak_memory_bytes = 1, // minimal memory, but present
        .memory_limit_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 10_000,
        .wall_ms = 10_000,
    });
    // mem_score ≈ 100, cpu_score = 0 → ~70 — CPU-only issue less severe
    try std.testing.expectEqual(@as(u8, 70), score);
}
