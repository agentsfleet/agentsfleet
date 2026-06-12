const std = @import("std");
const common = @import("common");
const http_server = @import("../http/server.zig");

var shutdown_requested = std.atomic.Value(bool).init(false);
var active_server = std.atomic.Value(?*http_server.Server).init(null);
var stop_server_fn: *const fn () void = defaultStopServer;
var stop_server_test_counter: ?*std.atomic.Value(u32) = null;

fn defaultStopServer() void {
    if (active_server.load(.acquire)) |s| s.stop();
}

pub fn reset() void {
    shutdown_requested.store(false, .release);
}

pub fn request() void {
    shutdown_requested.store(true, .release);
}

pub fn flag() *std.atomic.Value(bool) {
    return &shutdown_requested;
}

pub fn publishServer(server: *http_server.Server) void {
    active_server.store(server, .release);
}

pub fn clearServer() void {
    active_server.store(null, .release);
}

pub fn onSignal(sig: std.posix.SIG) callconv(.c) void {
    _ = sig;
    request();
}

pub fn signalWatcher() void {
    while (!shutdown_requested.load(.acquire)) {
        common.sleepNanos(100 * std.time.ns_per_ms);
    }
    stop_server_fn();
}

fn testStopServerHook() void {
    if (stop_server_test_counter) |counter| {
        _ = counter.fetchAdd(1, .acq_rel);
    }
}

test "integration: signalWatcher stops server on shutdown" {
    reset();

    var stop_calls = std.atomic.Value(u32).init(0);
    stop_server_test_counter = &stop_calls;
    stop_server_fn = testStopServerHook;
    defer {
        stop_server_fn = defaultStopServer;
        stop_server_test_counter = null;
        reset();
    }

    const thread = try std.Thread.spawn(.{}, signalWatcher, .{});
    common.sleepNanos(15 * std.time.ns_per_ms);
    request();
    thread.join();

    try std.testing.expectEqual(@as(u32, 1), stop_calls.load(.acquire));
}
