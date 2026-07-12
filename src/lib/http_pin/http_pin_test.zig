//! Fail-closed proofs for the shared TLS pin. The defect class this module
//! exists to prevent — a direct connect on unprimed certificate state
//! dereferencing a null validation clock — hid for months because every test
//! in the tree drove plain-http loopback URLs; these tests force the secure
//! branches deterministically, with no network: a failed prime must refuse
//! the pin, the refresh branch must not rescan, and URL refusals must fire
//! before any connect.
//!
//! std-only imports: this file compiles inside the `http_pin` module, whose
//! build-graph declaration carries no named-module dependencies.

const std = @import("std");
const testing = std.testing;
const http_pin = @import("http_pin.zig");

fn testIo() std.Io {
    return std.Io.Threaded.global_single_threaded.io();
}

test "a failed certificate rescan leaves the clock null and refuses the secure pin" {
    // fail_index 0: the bundle rescan's first allocation fails, so priming
    // cannot populate certificate state.
    var failing = testing.FailingAllocator.init(testing.allocator, .{ .fail_index = 0 });
    var client: std.http.Client = .{ .allocator = failing.allocator(), .io = testIo() };
    defer client.deinit();

    http_pin.primeTlsForDirectConnect(&client, client.io, true);
    try testing.expect(client.now == null);

    // The refusal fires on the null clock BEFORE any connect — pre-fix this
    // was the panic site (connect reading client.now.?), not a null return.
    try testing.expect(http_pin.pinPooledHandle(&client, "https://example.invalid/") == null);
    try testing.expect(http_pin.connectPinned(&client, "example.invalid", 443, true) == null);
}

test "a plain-http pin never touches certificate state" {
    var failing = testing.FailingAllocator.init(testing.allocator, .{ .fail_index = 0 });
    var client: std.http.Client = .{ .allocator = failing.allocator(), .io = testIo() };
    defer client.deinit();

    http_pin.primeTlsForDirectConnect(&client, client.io, false);
    try testing.expect(client.now == null);
    // No allocation was even attempted: the tls=false early-return is the
    // reason plain-http call sites never surfaced the panic in production.
    try testing.expect(!failing.has_induced_failure);
}

test "an already-primed client refreshes the clock without a rescan" {
    var failing = testing.FailingAllocator.init(testing.allocator, .{ .fail_index = 0 });
    var client: std.http.Client = .{ .allocator = failing.allocator(), .io = testIo() };
    defer client.deinit();

    client.now = std.Io.Clock.real.now(client.io);
    http_pin.primeTlsForDirectConnect(&client, client.io, true);
    // The refresh branch assigns the clock directly; a rescan would have hit
    // the failing allocator. Long-lived clients must keep validating rotated
    // certificates against current time without re-reading the trust store.
    try testing.expect(client.now != null);
    try testing.expect(!failing.has_induced_failure);
}

test "unusable URLs refuse the pin before any connect" {
    var client: std.http.Client = .{ .allocator = testing.allocator, .io = testIo() };
    defer client.deinit();

    try testing.expect(http_pin.pinPooledHandle(&client, "not a url") == null);
    try testing.expect(http_pin.connectPinned(&client, "", 80, false) == null);
}
