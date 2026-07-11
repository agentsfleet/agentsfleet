//! Process shutdown choreography: the signal flag, the watcher thread that
//! turns a signal into a server stop, and the streaming teardown sequence.
//!
//! Invariant (stop → join → deinit): nothing shared is freed while a thread
//! that touches it can still run. The watcher never retires on a shutdown
//! request alone — a signal landing BEFORE the server is published must stop
//! the server that is about to listen, so the watcher waits for the publish
//! and only exits after an actual stop (or an explicit teardown disarm).

const std = @import("std");
const common = @import("common");
const http_server = @import("../http/server.zig");
const subscription_hub = @import("../events/subscription_hub.zig");
const stream_registry = @import("../http/stream_registry.zig");

const WATCH_POLL_NS: u64 = 100 * std.time.ns_per_ms;

var shutdown_requested = std.atomic.Value(bool).init(false);
var watcher_disarmed = std.atomic.Value(bool).init(false);
var active_server = std.atomic.Value(?*http_server.Server).init(null);
var stop_server_fn: *const fn () bool = defaultStopServer;

/// Stop the published server. Returns false while no server is published —
/// the watcher keeps waiting instead of retiring on a no-op.
fn defaultStopServer() bool {
    const s = active_server.load(.acquire) orelse return false; // safe because: pairs with publishServer release-store.
    s.stop();
    return true;
}

pub fn reset() void {
    shutdown_requested.store(false, .release);
    watcher_disarmed.store(false, .release);
}

pub fn request() void {
    shutdown_requested.store(true, .release); // safe because: paired with .acquire loads in the watcher and background loops.
}

pub fn flag() *std.atomic.Value(bool) {
    return &shutdown_requested;
}

pub fn publishServer(server: *http_server.Server) void {
    active_server.store(server, .release); // safe because: paired with defaultStopServer's .acquire load.
}

pub fn clearServer() void {
    active_server.store(null, .release);
}

/// Retire the watcher without a server stop. Teardown calls this after the
/// server is already down (or never came up) and BEFORE joining the watcher
/// thread — otherwise a boot-failure join would hang on a watcher still
/// waiting for a server that will never be published.
pub fn disarmWatcher() void {
    watcher_disarmed.store(true, .release); // safe because: paired with the watcher's .acquire load.
}

pub fn onSignal(sig: std.posix.SIG) callconv(.c) void {
    _ = sig;
    request();
}

/// Runs until it has stopped a live server in response to a shutdown request,
/// or until disarmed by teardown. A request landing before the publish keeps
/// the watcher alive: it stops the server once it appears.
pub fn signalWatcher() void {
    while (!watcher_disarmed.load(.acquire)) { // safe because: pairs with disarmWatcher release-store.
        if (shutdown_requested.load(.acquire) and stop_server_fn()) return; // safe because: pairs with request() release-store.
        common.sleepNanos(WATCH_POLL_NS);
    }
}

/// Stream/hub teardown as one explicit sequence (was four LIFO defers in
/// serve.zig): stop's close broadcast wakes pop-parked stream threads (fd
/// shutdown alone cannot), awaitEmpty blocks until every stream thread has
/// deregistered, then the storage deinits — never freed under a live thread.
/// streams.drain() is deliberately NOT folded in: it is the first unwind step
/// (declared at the server site) so client fds shut down while srv.deinit()
/// is still joining request threads that may touch hub/registry.
pub fn deinitStreaming(hub: *subscription_hub, streams: *stream_registry) void {
    hub.stop();
    streams.awaitEmpty();
    hub.deinit();
    streams.deinit();
}

// ── tests ──────────────────────────────────────────────────────────────────

/// Test double for the server-stop hook: `published` gates whether a "server"
/// exists, mirroring the publish window the watcher must survive.
const TestHook = struct {
    var published = std.atomic.Value(bool).init(false);
    var stop_calls = std.atomic.Value(u32).init(0);
    var denied_once = common.Event{};
    var stopped = common.Event{};

    fn arm() void {
        published.store(false, .release);
        stop_calls.store(0, .release);
        denied_once = .{};
        stopped = .{};
        stop_server_fn = hook;
    }

    fn restore() void {
        stop_server_fn = defaultStopServer;
        reset();
    }

    fn hook() bool {
        if (!published.load(.acquire)) { // safe because: pairs with the test's release-store publish.
            denied_once.set();
            return false;
        }
        _ = stop_calls.fetchAdd(1, .acq_rel);
        stopped.set();
        return true;
    }
};

const HOOK_WAIT_NS: u64 = 2 * std.time.ns_per_s;

test "integration: signalWatcher stops server on shutdown" {
    reset();
    TestHook.arm();
    defer TestHook.restore();
    TestHook.published.store(true, .release);

    const thread = try std.Thread.spawn(.{}, signalWatcher, .{});
    request();
    try TestHook.stopped.timedWait(HOOK_WAIT_NS);
    thread.join();

    try std.testing.expectEqual(@as(u32, 1), TestHook.stop_calls.load(.acquire));
}

test "sigterm_before_publish_stops_server" {
    reset();
    TestHook.arm();
    defer TestHook.restore();

    const thread = try std.Thread.spawn(.{}, signalWatcher, .{});
    // Signal lands in the boot window: no server published yet.
    request();
    // The watcher observed the request with no server and kept waiting —
    // the pre-fix watcher exited here, permanently losing graceful shutdown.
    try TestHook.denied_once.timedWait(HOOK_WAIT_NS);
    // Boot completes: the server is published; the watcher must stop it.
    TestHook.published.store(true, .release);
    try TestHook.stopped.timedWait(HOOK_WAIT_NS);
    thread.join();

    try std.testing.expectEqual(@as(u32, 1), TestHook.stop_calls.load(.acquire));
}

test "disarm retires a watcher still waiting for a server" {
    reset();
    TestHook.arm();
    defer TestHook.restore();

    const thread = try std.Thread.spawn(.{}, signalWatcher, .{});
    request();
    try TestHook.denied_once.timedWait(HOOK_WAIT_NS);
    // Boot failed: no server will ever be published; teardown disarms.
    disarmWatcher();
    thread.join();

    try std.testing.expectEqual(@as(u32, 0), TestHook.stop_calls.load(.acquire));
}
