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

// The one bound the hotfix keeps: a POST whose cycle deadline is ALREADY spent
// is refused before any network I/O, cleanly, with `error.OtlpExportTimedOut`.
// This is the fail-fast pre-check inside postOnce — the only remaining deadline
// enforcement after the (crash-causing) mid-flight cancellation race was removed.
// The endpoint is a dead loopback port; a past deadline means the pre-check
// returns before fetch is even attempted, so it is never contacted (were the
// pre-check gone, this would instead surface a connect error, not a timeout —
// which is what makes this a regression guard for the surviving bound). Uses
// std.testing.allocator, so it doubles as a zero-leak proof of the timeout path
// (the URL scratch allocated before the check must be freed by its defer).
test "test_post_refuses_before_network_when_deadline_already_spent" {
    var threaded = std.Io.Threaded.init(std.heap.page_allocator, .{});
    defer threaded.deinit();
    const io = threaded.io();
    var client = Client.init(io);
    defer client.deinit();

    const cfg: config.GrafanaOtlpConfig = .{
        .endpoint = "http://127.0.0.1:1",
        .instance_id = "test-instance",
        .api_key = "test-key",
    };

    // -TEST_TIMEOUT_MS: as far in the PAST as TEST_TIMEOUT_MS is in the future.
    try std.testing.expectError(
        error.OtlpExportTimedOut,
        client.post(std.testing.allocator, cfg, "/v1/metrics", "{}", deadlineAfter(io, -TEST_TIMEOUT_MS)),
    );
}

/// Iterations over the fail-fast path; enough that a per-call leak accumulates
/// into something `std.testing.allocator` cannot miss at test end.
const LEAK_LOOP_ITERS: usize = 64;

// Cross-request no-growth leak proof for the reused persistent client. Each
// refused POST allocates its URL scratch and must free it via defer on the
// early-return path; a persistent-client pool that retained per-call state
// would show up as a leak accumulated over the loop. std.testing.allocator
// fails the test on the first unfreed byte, so a clean run over N iterations is
// the proof the fail-fast path leaks nothing across reuse.
test "test_post_fail_fast_path_leaks_nothing_across_reuse" {
    var threaded = std.Io.Threaded.init(std.heap.page_allocator, .{});
    defer threaded.deinit();
    const io = threaded.io();
    var client = Client.init(io);
    defer client.deinit();

    const cfg: config.GrafanaOtlpConfig = .{
        .endpoint = "http://127.0.0.1:1",
        .instance_id = "test-instance",
        .api_key = "test-key",
    };

    var i: usize = 0;
    while (i < LEAK_LOOP_ITERS) : (i += 1) {
        try std.testing.expectError(
            error.OtlpExportTimedOut,
            client.post(std.testing.allocator, cfg, "/v1/metrics", "{}", deadlineAfter(io, -TEST_TIMEOUT_MS)),
        );
    }
}

// Minimal HTTP/1.1 200 with no body: Content-Length 0 so the client's response
// read completes without a body. The one response the OTLP happy path expects.
const OK_RESPONSE = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
const SERVER_READ_BUF: usize = 1024;
const SERVER_WRITE_BUF: usize = 128;

/// One-shot loopback server: accept, drain the request so the client's send
/// completes, answer 200, close. `listenLoopback` hands back an already-bound
/// held listener, so there is no close-and-rebind race (harness hygiene).
const OkServer = struct {
    io: std.Io,
    listener: *std.Io.net.Server,

    fn run(self: *OkServer) void {
        const stream = self.listener.accept(self.io) catch return;
        defer stream.close(self.io);
        var rbuf: [SERVER_READ_BUF]u8 = undefined;
        // Drain the request so the client's send completes; a broken read means
        // the client already went away, so give up rather than answer nothing.
        _ = std.posix.read(stream.socket.handle, &rbuf) catch return;
        var wbuf: [SERVER_WRITE_BUF]u8 = undefined;
        var w = stream.writer(self.io, &wbuf);
        w.interface.writeAll(OK_RESPONSE) catch return;
        w.interface.flush() catch return;
    }
};

// The real-fetch happy path, which the injected `post` seam in exporter_test can
// never cover: a POST that actually leaves the socket, gets a 200, and parses an
// empty body into `.accepted`. Exercises real send + real response-head read +
// parseResponse end to end against a live (loopback) server. Not a crash
// regression — plain HTTP answering immediately can't reproduce the TLS
// send-phase timing — but it is the positive scenario-axis case for the fetch
// path the exporter runs in production. testing.allocator doubles it as a
// zero-leak proof of the success path (URL + response scratch freed).
test "integration: test_post_returns_accepted_on_200" {
    var threaded = std.Io.Threaded.init(std.heap.page_allocator, .{});
    defer threaded.deinit();
    const io = threaded.io();
    var loopback = test_port.listenLoopback(io) catch return error.SkipZigTest;
    defer loopback.server.deinit(io);

    var server = OkServer{ .io = io, .listener = &loopback.server };
    const server_thread = try std.Thread.spawn(.{}, OkServer.run, .{&server});
    defer server_thread.join();

    var endpoint_buf: [64]u8 = undefined;
    const endpoint = try std.fmt.bufPrint(&endpoint_buf, "http://127.0.0.1:{d}", .{loopback.port});
    const cfg: config.GrafanaOtlpConfig = .{
        .endpoint = endpoint,
        .instance_id = "test-instance",
        .api_key = "test-key",
    };
    var client = Client.init(io);
    defer client.deinit();

    const result = try client.post(std.testing.allocator, cfg, "/v1/metrics", "{}", deadlineAfter(io, TEST_TIMEOUT_MS));
    try std.testing.expect(result == .accepted);
}

// A stall-times-out test also lived here. It drove a loopback peer that accepted
// the connection and sent nothing, and asserted `post` escaped via its deadline —
// but the mechanism it proved (racing the fetch and canceling it mid-flight) is
// exactly what crashed the process against a real TLS endpoint, so it was removed
// with that mechanism. That test stalled on the RESPONSE read, after the send had
// completed, which is why it never reproduced the send-phase panic even on the
// buggy code. Restore a stall test alongside the cancel-safe bound (shut the
// pinned socket down at the deadline) when that lands.
