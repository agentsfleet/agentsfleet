// rpc_version mismatch fast-fail test for the executor RPC handshake.
//
// Spawns the harness binary with `EXECUTOR_HARNESS_RPC_VERSION=1` so the
// server side advertises (and expects) v1 in its HELLO frame. We then
// drive the lower-level transport.Client at v2 (no override on this side)
// so the handshake mismatches in both directions: server sees v2 from
// client, client sees v1 from server, both fail with RpcVersionMismatch
// within the connect call. No StartStage RPCs cross the wire.
//
// Uses transport.Client.connect rather than the worker's ExecutorClient so
// the failure path doesn't surface a log.err — Zig's test framework treats
// any log.err during a test as a failure. This mirrors the pattern in
// src/executor/crash_test.zig.
//
// Requires the harness binary; this test does not seed Postgres or Redis.

const std = @import("std");
const Allocator = std.mem.Allocator;

const transport = @import("../executor/transport.zig");
const Harness = @import("test_executor_harness.zig");
const helpers = @import("test_harness_helpers.zig");

const ALLOC = std.testing.allocator;

// SLA from the spec: both sides log + abort the connection within 100 ms.
// Allow a generous ceiling for CI scheduling jitter; the actual wall time
// in practice is well under 50 ms.
const ABORT_BUDGET_MS: i64 = 1_500;

const MISMATCH_RPC_VERSION: u32 = 1;

test "integration: harness with rpc_version=1 fails transport.Client.connect fast" {
    if (std.process.getEnvVarOwned(ALLOC, helpers.SKIP_ENV_VAR)) |s| {
        defer ALLOC.free(s);
        return error.SkipZigTest;
    } else |_| {}

    var harness = Harness.start(ALLOC, .{
        .rpc_version = MISMATCH_RPC_VERSION,
        .auto_connect = false,
    }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer harness.deinit();

    var tc = transport.Client.init(ALLOC, harness.socket_path);
    defer tc.close();

    const t_start = std.time.milliTimestamp();
    const result = tc.connect();
    const elapsed = std.time.milliTimestamp() - t_start;

    // Both peers see a HELLO whose advertised version doesn't match their
    // expected version → RpcVersionMismatch on the client side.
    try std.testing.expectError(transport.ConnectionError.RpcVersionMismatch, result);
    try std.testing.expect(elapsed < ABORT_BUDGET_MS);
}
