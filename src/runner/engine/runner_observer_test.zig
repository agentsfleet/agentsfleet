//! Tests for runner_observer.zig — env-var parsing and backend dispatch.

const std = @import("std");
const ObserverRuntime = @import("runner_observer.zig");
const common = @import("common");

test "parseBackend defaults to log_backend on garbage" {
    try std.testing.expectEqual(@as(ObserverRuntime.ObserverBackend, .log_backend), ObserverRuntime.parseBackend("garbage"));
    try std.testing.expectEqual(@as(ObserverRuntime.ObserverBackend, .log_backend), ObserverRuntime.parseBackend(""));
    try std.testing.expectEqual(@as(ObserverRuntime.ObserverBackend, .log_backend), ObserverRuntime.parseBackend("LOG_BACKEND"));
}

test "parseBackend matches 'noop' case-insensitively" {
    try std.testing.expectEqual(@as(ObserverRuntime.ObserverBackend, .noop), ObserverRuntime.parseBackend("noop"));
    try std.testing.expectEqual(@as(ObserverRuntime.ObserverBackend, .noop), ObserverRuntime.parseBackend("NOOP"));
    try std.testing.expectEqual(@as(ObserverRuntime.ObserverBackend, .noop), ObserverRuntime.parseBackend("NoOp"));
    try std.testing.expectEqual(@as(ObserverRuntime.ObserverBackend, .noop), ObserverRuntime.parseBackend("nOoP"));
}

test "parseBackend matches 'verbose' case-insensitively" {
    try std.testing.expectEqual(@as(ObserverRuntime.ObserverBackend, .verbose), ObserverRuntime.parseBackend("verbose"));
    try std.testing.expectEqual(@as(ObserverRuntime.ObserverBackend, .verbose), ObserverRuntime.parseBackend("VERBOSE"));
    try std.testing.expectEqual(@as(ObserverRuntime.ObserverBackend, .verbose), ObserverRuntime.parseBackend("VeRbOsE"));
}

test "init falls back to log_backend when NULLCLAW_OBSERVER is unset" {
    // Synthetic empty env map → NULLCLAW_OBSERVER is definitively unset, so
    // init must take the fallback branch. 0.16 threads the environment via
    // Environ.Map instead of a live process read, which makes this
    // deterministic (no more "might be set in CI" hedge).
    var env_map = try common.env.fromPairs(std.testing.allocator, &.{});
    defer env_map.deinit();
    const rt = ObserverRuntime.init(&env_map);
    try std.testing.expectEqual(ObserverRuntime.ObserverBackend.log_backend, rt.backend);
}

test "observer() returns the backend's observer for each variant" {
    inline for (.{ ObserverRuntime.ObserverBackend.log_backend, .noop, .verbose }) |b| {
        var rt = ObserverRuntime{ .backend = b };
        // Smoke test: observer() compiles + returns a non-undefined Observer
        // for every backend variant. The actual call shape is
        // nullclaw.observability.Observer — we only assert the dispatch
        // doesn't trap.
        _ = rt.observer();
    }
}
