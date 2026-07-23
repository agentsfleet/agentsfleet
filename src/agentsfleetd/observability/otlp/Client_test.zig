const std = @import("std");
const common = @import("common");
const Client = @import("Client.zig");
const config = @import("config.zig");
const test_port = @import("../../http/test_port.zig");

const TEST_TIMEOUT_MS: i96 = 1_000;

fn deadlineAfter(io: std.Io, timeout_ms: i96) i96 {
    return std.Io.Clock.boot.now(io).toNanoseconds() + timeout_ms * std.time.ns_per_ms;
}

// The persistent client's "one client, reused across every flush" guarantee is
// structural: the flush loop calls Client.init() once before its while loop and
// reuses that instance, and the real POST is exercised by the integration path
// (a unit test must not depend on network connectivity). Here we assert the
// construct → tear-down lifecycle is sound (no crash, no leaked client state).
test "test_persistent_client_lifecycle: construct and tear down without crash" {
    var client = Client.init(common.globalIo());
    client.deinit();
}

// §3 / Dimension 3.1 — a transport failure must PROPAGATE as an error,
// not `catch return;` into a bare success. Before the fix the fetch's
// `catch return;` made a connection-refused (or DNS/TLS) failure look identical
// to a 2xx export, so `flushOnce`'s `catch |err| log.warn(EVENT_IGNORED_ERROR)`
// never fired and an OTLP outage was silent. The fault is injected the realistic
// way — a loopback port with nothing listening yields a refused connect — and we
// assert `post` returns *some* error the exporter's catch-warn can log (the exact
// error kind is std/platform-dependent, so we assert propagation, not identity).
//
// Uses std.testing.allocator: post() now frees its URL + response scratch via
// `defer` on every path, so this doubles as a zero-leak proof on the error path.
test "test_post_propagates_transport_error_to_exporter_log" {
    const alloc = std.testing.allocator;

    var threaded = std.Io.Threaded.init(std.heap.page_allocator, .{});
    defer threaded.deinit();
    const io = threaded.io();
    var client = Client.init(io);
    defer client.deinit();

    // 127.0.0.1:1 — a privileged port with no listener; connect() is refused on
    // loopback, so the failure is deterministic and fast.
    const cfg: config.GrafanaOtlpConfig = .{
        .endpoint = "http://127.0.0.1:1",
        .instance_id = "test-instance",
        .api_key = "test-key",
    };

    if (client.post(alloc, cfg, "/v1/metrics", "{}", deadlineAfter(io, TEST_TIMEOUT_MS))) |_| {
        // No error returned → the transport failure was swallowed as a bare
        // success (the pre-fix `catch return;` behaviour). That is the bug.
        return error.TransportFailureSwallowedAsSuccess;
    } else |_| {
        // Expected: the transport error propagated so flushOnce can warn-log it.
    }
}

// §6 — the URL/auth *formatting* failures (originally deferred `catch
// return;` swallows on lines 39/43/51) now propagate too, not just the fetch.
// Deterministic injection: an `instance_id` longer than the 512-byte auth buffer
// makes the Basic-auth `bufPrint` overflow, which pre-fix silently returned a bare
// success. We assert it now surfaces `error.NoSpaceLeft` (testing.allocator also
// confirms the URL scratch allocated before the overflow is freed by its defer).
test "test_post_propagates_oversized_auth_formatting_error" {
    const alloc = std.testing.allocator;

    var threaded = std.Io.Threaded.init(std.heap.page_allocator, .{});
    defer threaded.deinit();
    const io = threaded.io();
    var client = Client.init(io);
    defer client.deinit();

    const oversized_instance_id = "x" ** 600; // > the 512-byte auth_raw_buf
    const cfg: config.GrafanaOtlpConfig = .{
        .endpoint = "http://127.0.0.1:1",
        .instance_id = oversized_instance_id,
        .api_key = "k",
    };

    try std.testing.expectError(
        error.NoSpaceLeft,
        client.post(alloc, cfg, "/v1/metrics", "{}", deadlineAfter(io, TEST_TIMEOUT_MS)),
    );
}

/// Upper bound on how long the stalling peer holds its accepted connection —
/// a backstop so a failed test cannot wedge the suite, never the timing under test.
const STALL_HOLD_TIMEOUT_NS: u64 = 5 * std.time.ns_per_s;

const StallServer = struct {
    io: std.Io,
    listener: *std.Io.net.Server,
    release: common.Event = .{},

    fn run(self: *StallServer) void {
        const stream = self.listener.accept(self.io) catch return;
        defer stream.close(self.io);
        // Hold the accepted connection open and send nothing, so the client can
        // only escape via its own deadline. Timing out here means the test
        // already released us; close the stream either way.
        self.release.timedWait(STALL_HOLD_TIMEOUT_NS) catch |err| switch (err) {
            error.Timeout => {},
        };
    }
};

test "integration: test_otlp_post_stall_times_out" {
    var threaded = std.Io.Threaded.init(std.heap.page_allocator, .{});
    defer threaded.deinit();
    const io = threaded.io();
    var loopback = test_port.listenLoopback(io) catch return error.SkipZigTest;
    defer loopback.server.deinit(io);

    var server = StallServer{ .io = io, .listener = &loopback.server };
    const server_thread = try std.Thread.spawn(.{}, StallServer.run, .{&server});
    defer {
        server.release.set();
        server_thread.join();
    }

    var endpoint_buf: [64]u8 = undefined;
    const endpoint = try std.fmt.bufPrint(
        &endpoint_buf,
        "http://127.0.0.1:{d}",
        .{loopback.port},
    );
    const cfg: config.GrafanaOtlpConfig = .{
        .endpoint = endpoint,
        .instance_id = "test-instance",
        .api_key = "test-key",
    };
    var client = Client.init(io);
    defer client.deinit();

    const start = std.Io.Clock.boot.now(io).toNanoseconds();
    try std.testing.expectError(
        error.OtlpExportTimedOut,
        client.post(
            std.testing.allocator,
            cfg,
            "/v1/metrics",
            "{}",
            deadlineAfter(io, 100),
        ),
    );
    const elapsed = std.Io.Clock.boot.now(io).toNanoseconds() - start;
    try std.testing.expect(elapsed < 2 * std.time.ns_per_s);
}
