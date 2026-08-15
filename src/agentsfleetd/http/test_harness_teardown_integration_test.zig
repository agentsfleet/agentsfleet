//! Integration tier for the harness's own shutdown path — the teardown every
//! other suite in this directory depends on to return.
//!
//! `TestHarness.start`'s failure path and `deinit` both stop the accept loop and
//! join its thread. An unbounded join there parks the whole test binary whenever
//! `stop()` fails to wake `accept()`: every test that would have run afterwards
//! is lost, and the process prints nothing to say why — it simply sits at 0%
//! CPU. These assert the signal that makes the wait bounded, and that the happy
//! path pays nothing for it.

const std = @import("std");
const common = @import("common");
const harness_mod = @import("test_harness.zig");
const auth_mw = @import("../auth/middleware/mod.zig");

const TestHarness = harness_mod.TestHarness;

/// Comfortably under the 5 s teardown timeout and far above a real teardown,
/// which is sub-millisecond. A deinit slower than this means the poll loop is
/// spinning on a flag that never flipped — the same defect as the hang, just
/// close enough to the deadline to pass by luck.
const TEARDOWN_BUDGET_MS: i64 = 2_000;

/// The suite exercises lifecycle only, so the registry keeps its stub defaults.
fn noRegistryChanges(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

test "integration: the accept loop reports itself running, then exits on teardown" {
    const alloc = std.testing.allocator;
    const h = try TestHarness.start(alloc, .{ .configureRegistry = noRegistryChanges });

    // Non-vacuous first. `start` returned, so the server answered /healthz and
    // the loop is live — the flag must still be false, or teardown's wait would
    // be trivially satisfied and would prove nothing about a real shutdown.
    try std.testing.expect(!h.listen_returned.load(.seq_cst));

    // Monotonic: a wall-clock read would let an NTP step masquerade as a stall.
    const started_ms = common.clock.nowMonotonicMillis();
    h.deinit();
    const elapsed_ms = common.clock.nowMonotonicMillis() - started_ms;

    // deinit returning at all is the assertion that the flag flipped: the
    // bounded wait panics rather than returning if the loop never exits. The
    // budget catches the near-miss the panic would not.
    try std.testing.expect(elapsed_ms < TEARDOWN_BUDGET_MS);
}
