//! Process shutdown choreography: the signal flag, the watcher thread that
//! turns a signal into a server stop, and the streaming teardown sequence.
//!
//! Invariant (stop → join → deinit): nothing shared is freed while a thread
//! that touches it can still run. The watcher never retires on a shutdown
//! request alone — a signal landing BEFORE the server is published must stop
//! the server that is about to listen, so the watcher waits for the publish
//! and only exits after an actual stop (or an explicit teardown disarm).
//!
//! Two flags, deliberately split: `shutdown_requested` (the raw signal, read
//! only by the watcher) and the background-stop flag handed to the sweepers /
//! outbound worker via `flag()`. A boot-window signal must not kill the
//! background stack while the server can still come up and briefly serve —
//! background loops stop only when the watcher actually stops a published
//! server, or at teardown disarm.

const std = @import("std");
const common = @import("common");
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");
const http_server = @import("../http/server.zig");
const subscription_hub = @import("../events/subscription_hub.zig");
const fleet_set_cache = @import("../events/fleet_set_cache.zig");
const stream_registry = @import("../http/stream_registry.zig");

const log = logging.scoped(.serve_shutdown);

const WATCH_POLL_NS: u64 = 100 * std.time.ns_per_ms;

var shutdown_requested = std.atomic.Value(bool).init(false);
var watcher_disarmed = std.atomic.Value(bool).init(false);
var background_stop = std.atomic.Value(bool).init(false);
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
    background_stop.store(false, .release);
}

pub fn request() void {
    shutdown_requested.store(true, .release); // safe because: paired with the watcher's .acquire load.
}

/// The background loops' stop flag (sweepers, outbound worker) — NOT the raw
/// signal flag. It flips when the watcher stops a published server or at
/// teardown disarm, so a pre-publish signal never strands a serving node
/// without its background stack.
pub fn flag() *std.atomic.Value(bool) {
    return &background_stop;
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
/// waiting for a server that will never be published. Also releases the
/// background loops: teardown joins them right after this, so they must see
/// stop even when no server stop ever happened (boot failure).
pub fn disarmWatcher() void {
    background_stop.store(true, .release); // safe because: paired with the background loops' .acquire loads.
    watcher_disarmed.store(true, .release); // safe because: paired with the watcher's .acquire load.
}

pub fn onSignal(sig: std.posix.SIG) callconv(.c) void {
    _ = sig;
    request();
}

/// Runs until it has stopped a live server in response to a shutdown request,
/// or until disarmed by teardown. A request landing before the publish keeps
/// the watcher alive: it stops the server once it appears. Only an ACTUAL
/// server stop releases the background loops — see the flag() doc.
pub fn signalWatcher() void {
    while (!watcher_disarmed.load(.acquire)) { // safe because: pairs with disarmWatcher release-store.
        if (shutdown_requested.load(.acquire) and stop_server_fn()) { // safe because: pairs with request() release-store.
            background_stop.store(true, .release); // safe because: paired with the background loops' .acquire loads.
            return;
        }
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
pub fn deinitStreaming(hub: *subscription_hub, streams: *stream_registry, fleet_sets: *fleet_set_cache) void {
    hub.stop();
    streams.awaitEmpty();
    hub.deinit();
    streams.deinit();
    // After awaitEmpty: every stream thread has deregistered, so no workspace
    // stream can still hold a fleet-set reference. Freeing it before that would
    // free the set under a live tick.
    fleet_sets.deinit();
}

/// One wait round on the detached install workers before logging stragglers.
const INSTALL_WAIT_ROUND_MS: u64 = 10_000;
const INSTALL_WAIT_POLL_MS: u64 = 25;

/// Teardown wait for the detached install-step workers. Never proceeds under
/// a live worker (the pool/queue they borrow deinit right after this — a loud
/// hang systemd escalates beats a use-after-free), but unlike a bare
/// WaitGroup.wait() each expired round warns with the straggler count, so a
/// shutdown blocked on a hung install query is visible in the journal.
pub fn awaitInstallWorkers(wg: *common.WaitGroup) void {
    var waited_ms: u64 = 0;
    var rounds: u32 = 0;
    while (wg.pending() != 0) {
        common.sleepNanos(INSTALL_WAIT_POLL_MS * std.time.ns_per_ms);
        waited_ms += INSTALL_WAIT_POLL_MS;
        if (waited_ms >= INSTALL_WAIT_ROUND_MS) {
            rounds += 1;
            log.warn("install_wait_incomplete", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .pending = wg.pending(), .rounds = rounds });
            waited_ms = 0;
        }
    }
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
    // The raw signal must NOT have stopped the background loops: the node is
    // still booting and may yet serve — killing the sweepers here is the
    // half-dead-node bug.
    try std.testing.expect(!flag().load(.acquire));
    // Boot completes: the server is published; the watcher must stop it.
    TestHook.published.store(true, .release);
    try TestHook.stopped.timedWait(HOOK_WAIT_NS);
    thread.join();

    try std.testing.expectEqual(@as(u32, 1), TestHook.stop_calls.load(.acquire));
    // The actual server stop is what releases the background loops.
    try std.testing.expect(flag().load(.acquire));
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
    // Disarm releases the background loops so teardown's joins can't hang.
    try std.testing.expect(flag().load(.acquire));
}

test "awaitInstallWorkers returns once the last detached worker finishes" {
    var wg: common.WaitGroup = .{};
    wg.start();
    const worker = try std.Thread.spawn(.{}, struct {
        fn run(w: *common.WaitGroup) void {
            common.sleepNanos(30 * std.time.ns_per_ms);
            w.finish();
        }
    }.run, .{&wg});
    awaitInstallWorkers(&wg);
    worker.join();
    try std.testing.expectEqual(@as(usize, 0), wg.pending());
}
