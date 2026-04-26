//! Lifecycle tests for the watcher's per-zombie runtime support.
//!
//! Two pre-existing watcher bugs surfaced by adversarial review:
//!
//!   - cancel_flags entry leaked on Thread.spawn failure → permanently
//!     stuck zombie (next reconcile saw `contains == true`, skipped).
//!   - Per-zombie thread that exited early (Redis connect / claim / no
//!     executor) left its map entry behind for the worker process's life,
//!     so the same zombie_id could never re-spawn.
//!
//! Fixes verified here:
//!
//!   - sweepExitedLocked reaps entries whose runtime.exited is true,
//!     leaves running entries alone, and frees keys + runtime + detaches
//!     thread handles in lockstep.
//!   - ZombieRuntime.init starts both atomics false.
//!
//! Lock-acquisition order (asserted in prose; the watcher has exactly one
//! lock surface, so the property is structural rather than runtime):
//!
//!   1. map_lock is the only lock the watcher takes.
//!   2. spawnZombieThread holds map_lock across sweepExitedLocked + map
//!      inserts. sweepExitedLocked never re-acquires (caller-locked).
//!   3. cancelZombie holds map_lock across runtimes.get +
//!      runtime.cancel.store. Released BEFORE the executor RPC (which
//!      does Redis I/O and may block).
//!   4. zombieRuntimeWrapper does NOT touch map_lock — only the
//!      per-runtime exited atomic.
//!   5. deinit runs single-threaded after the run loop exits.
//!
//! With one lock and no nested acquisitions there is no possible deadlock
//! ordering — concurrent cancelZombie + spawnZombieThread + wrapper-exit
//! merely serialise on map_lock, never block on a second lock.

const std = @import("std");
const runtime_mod = @import("worker_watcher_runtime.zig");

test "unit: ZombieRuntime.init starts both atomics false" {
    var rt = runtime_mod.ZombieRuntime.init();
    try std.testing.expect(!rt.cancel.load(.acquire));
    try std.testing.expect(!rt.exited.load(.acquire));
}

fn noopThread() void {}

test "unit: sweepExitedLocked reaps exited entries, preserves running ones" {
    const alloc = std.testing.allocator;

    var runtimes = std.StringHashMap(*runtime_mod.ZombieRuntime).init(alloc);
    defer runtimes.deinit();
    var threads = std.StringHashMap(std.Thread).init(alloc);
    defer threads.deinit();

    // Live entry: exited=false. Sweep must NOT touch it.
    const rt_alive = try alloc.create(runtime_mod.ZombieRuntime);
    rt_alive.* = runtime_mod.ZombieRuntime.init();
    const k_runtimes_alive = try alloc.dupe(u8, "alive-id");
    try runtimes.put(k_runtimes_alive, rt_alive);

    // Dead entry: exited=true + a thread that has already returned. Sweep
    // must remove from both maps and detach the (already-exited) handle.
    const rt_dead = try alloc.create(runtime_mod.ZombieRuntime);
    rt_dead.* = runtime_mod.ZombieRuntime.init();
    rt_dead.exited.store(true, .release);
    const k_runtimes_dead = try alloc.dupe(u8, "dead-id");
    try runtimes.put(k_runtimes_dead, rt_dead);
    const dead_thread = try std.Thread.spawn(.{}, noopThread, .{});
    const k_threads_dead = try alloc.dupe(u8, "dead-id");
    try threads.put(k_threads_dead, dead_thread);

    try runtime_mod.sweepExitedLocked(alloc, &runtimes, &threads);

    try std.testing.expect(runtimes.contains("alive-id"));
    try std.testing.expect(!runtimes.contains("dead-id"));
    try std.testing.expect(!threads.contains("dead-id"));

    // Reap the still-live entry so std.testing.allocator's leak gate is
    // satisfied. (Production deinit does this for every remaining entry.)
    if (runtimes.fetchRemove("alive-id")) |kv| {
        alloc.free(kv.key);
        alloc.destroy(kv.value);
    }
}

test "unit: sweepExitedLocked is a no-op on empty maps" {
    const alloc = std.testing.allocator;
    var runtimes = std.StringHashMap(*runtime_mod.ZombieRuntime).init(alloc);
    defer runtimes.deinit();
    var threads = std.StringHashMap(std.Thread).init(alloc);
    defer threads.deinit();

    try runtime_mod.sweepExitedLocked(alloc, &runtimes, &threads);

    try std.testing.expectEqual(@as(usize, 0), runtimes.count());
    try std.testing.expectEqual(@as(usize, 0), threads.count());
}

fn flipExitedThread(rt: *runtime_mod.ZombieRuntime) void {
    rt.exited.store(true, .release);
}

test "unit: wrapper-style exit signal is observable + sweepable" {
    // Mirrors what zombieRuntimeWrapper does at the end of its body
    // (worker_watcher_runtime.zig line ≈58). The unit test does NOT call
    // worker_zombie.zombieWorkerLoop because that needs a live PG pool.
    // Instead a noop thread flips exited directly — same observable
    // post-state from the watcher's perspective.
    const alloc = std.testing.allocator;
    var runtimes = std.StringHashMap(*runtime_mod.ZombieRuntime).init(alloc);
    defer runtimes.deinit();
    var threads = std.StringHashMap(std.Thread).init(alloc);
    defer threads.deinit();

    const rt = try alloc.create(runtime_mod.ZombieRuntime);
    rt.* = runtime_mod.ZombieRuntime.init();
    const k_rt = try alloc.dupe(u8, "z-1");
    try runtimes.put(k_rt, rt);

    const t = try std.Thread.spawn(.{}, flipExitedThread, .{rt});
    const k_thr = try alloc.dupe(u8, "z-1");
    try threads.put(k_thr, t);

    // Wait for the wrapper-style flip. 100ms is generous given the noop
    // thread's body is a single atomic store.
    var spins: u32 = 0;
    while (!rt.exited.load(.acquire)) : (spins += 1) {
        if (spins > 1_000) return error.WrapperFlipTimeout;
        std.Thread.sleep(100 * std.time.ns_per_us);
    }

    try runtime_mod.sweepExitedLocked(alloc, &runtimes, &threads);

    try std.testing.expect(!runtimes.contains("z-1"));
    try std.testing.expect(!threads.contains("z-1"));
}
