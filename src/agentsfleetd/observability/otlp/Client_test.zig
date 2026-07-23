const std = @import("std");
const common = @import("common");
const Client = @import("Client.zig");
const config = @import("config.zig");

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

    var client = Client.init(common.globalIo());
    defer client.deinit();

    // 127.0.0.1:1 — a privileged port with no listener; connect() is refused on
    // loopback, so the failure is deterministic and fast.
    const cfg: config.GrafanaOtlpConfig = .{
        .endpoint = "http://127.0.0.1:1",
        .instance_id = "test-instance",
        .api_key = "test-key",
    };

    if (client.post(alloc, cfg, "/v1/metrics", "{}")) |_| {
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

    var client = Client.init(common.globalIo());
    defer client.deinit();

    const oversized_instance_id = "x" ** 600; // > the 512-byte auth_raw_buf
    const cfg: config.GrafanaOtlpConfig = .{
        .endpoint = "http://127.0.0.1:1",
        .instance_id = oversized_instance_id,
        .api_key = "k",
    };

    try std.testing.expectError(error.NoSpaceLeft, client.post(alloc, cfg, "/v1/metrics", "{}"));
}
