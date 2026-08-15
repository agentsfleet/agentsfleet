//! Failure-arm proofs for the engine's `execute` entry point (runner.zig) — the
//! two refusals that resolve before any provider or network exists, offline and
//! deterministic. The success mapping needs a live provider and belongs to the
//! stubbed-provider integration path (`run_context_test.zig` drives the inner
//! seam; the full arc is integration-lane).

const std = @import("std");
const testing = std.testing;
const runner = @import("runner.zig");

const ALLOC = testing.allocator;
const WORKSPACE = "/tmp/agentsfleet-m164-engine-execute";
// pin test: literal is the contract — the wire-visible failure details.
const EXPECTED_MISSING_MESSAGE_DETAIL = "lease carried no message to run";
const EXPECTED_INIT_FAILURE_DETAIL = "FleetInitFailed";

test "a lease with no message refuses as an invalid config, never dials a provider" {
    var env_map: std.process.Environ.Map = .init(ALLOC);
    defer env_map.deinit();
    const result = runner.execute(&env_map, ALLOC, WORKSPACE, null, null, null, null, null, null, &.{}, null);
    // Static detail — nothing allocated, nothing to free, nothing dialed.
    try testing.expect(!result.succeeded());
    try testing.expectEqualStrings(EXPECTED_MISSING_MESSAGE_DETAIL, result.failureDetail());
    try testing.expectEqual(@as(usize, 0), result.content.len);
}

test "a config that cannot even load maps onto a reportable failure result" {
    var env_map: std.process.Environ.Map = .init(ALLOC);
    defer env_map.deinit();
    // The first allocation of the config build fails: `execute` must catch the
    // inner error and map it to a failed ExecutionResult carrying the error
    // name — the parent reports it; a crash here would take the worker down.
    var failing = std.testing.FailingAllocator.init(ALLOC, .{ .fail_index = 0 });
    const result = runner.execute(&env_map, failing.allocator(), WORKSPACE, null, null, "run the fleet", null, null, null, &.{}, null);
    try testing.expect(!result.succeeded());
    try testing.expectEqualStrings(EXPECTED_INIT_FAILURE_DETAIL, result.failureDetail());
    try testing.expectEqual(@as(usize, 0), result.content.len);
}
