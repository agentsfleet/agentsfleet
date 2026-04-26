//! Per-zombie runtime support for the worker watcher.
//!
//! Owns the `ZombieRuntime` struct (cancel + exited atomics), the watcher-
//! controlled thread entry `zombieRuntimeWrapper`, and `sweepExitedLocked`
//! (called by the watcher's `spawnZombieThread` while holding `map_lock`).
//!
//! Lives separately from `worker_watcher.zig` to keep that file under the
//! 350-line cap; semantically these are watcher internals — only the
//! watcher constructs and frees these values.
//!
//! Lock-acquisition order (the watcher has exactly one lock):
//!
//!   1. `Watcher.map_lock` is the only lock the watcher takes.
//!   2. `spawnZombieThread` acquires `map_lock`, calls `sweepExitedLocked`
//!      under the same lock (no re-entry), inserts new entries, releases.
//!   3. `cancelZombie` acquires `map_lock`, reads + writes the cancel atomic
//!      under the lock, releases, THEN does executor RPC outside the lock.
//!   4. `zombieRuntimeWrapper` does NOT acquire `map_lock`. It only flips a
//!      per-runtime atomic on return. The wrapper running concurrently with
//!      any of the above is by design lock-free.
//!   5. `deinit` runs after every wrapper has been observed exited (caller
//!      must signal shutdown and join). It iterates and frees without the
//!      lock — there is no other thread to race with at that point.

const std = @import("std");
const worker_zombie = @import("worker_zombie.zig");

/// Per-zombie runtime owned by the watcher map. The cancel atomic drives
/// the `worker_zombie.watchShutdown` poll; the exited atomic is set once
/// by `zombieRuntimeWrapper` after `worker_zombie.zombieWorkerLoop` returns
/// and is read by `sweepExitedLocked` to decide which entries are reapable.
pub const ZombieRuntime = struct {
    cancel: std.atomic.Value(bool),
    exited: std.atomic.Value(bool),

    pub fn init() ZombieRuntime {
        return .{
            .cancel = std.atomic.Value(bool).init(false),
            .exited = std.atomic.Value(bool).init(false),
        };
    }
};

/// Thread entry point for a per-zombie worker.
///
/// Calls `worker_zombie.zombieWorkerLoop` and on return flips
/// `runtime.exited`. The watcher's next `spawnZombieThread` (driven either
/// by a control-stream `zombie_created` retry or the periodic reconcile
/// sweep) calls `sweepExitedLocked` to reap the runtime.
///
/// Does NOT touch the watcher's maps directly — keeps lock-acquisition
/// order trivial and avoids blurring map ownership.
pub fn zombieRuntimeWrapper(
    runtime: *ZombieRuntime,
    alloc: std.mem.Allocator,
    cfg: worker_zombie.ZombieWorkerConfig,
) void {
    worker_zombie.zombieWorkerLoop(alloc, cfg);
    runtime.exited.store(true, .release);
}

/// Reap entries whose wrapper has flipped `runtime.exited`. Caller MUST
/// hold the watcher's `map_lock`. Two-pass walk (collect then mutate) so
/// the iterator stays valid across removals.
///
/// On reap: removes from both maps, frees the duped key bytes, destroys
/// the runtime, and `Thread.detach()`s the (already-exited) handle so the
/// kernel reclaims the pthread storage without a `join`. Calling
/// `Thread.detach()` on a handle whose underlying thread has already
/// returned is well-defined on POSIX — the join slot is released, the
/// pthread_t becomes invalid, and the OS reaps the thread record.
pub fn sweepExitedLocked(
    alloc: std.mem.Allocator,
    runtimes: *std.StringHashMap(*ZombieRuntime),
    threads: *std.StringHashMap(std.Thread),
) !void {
    var to_remove: std.ArrayList([]const u8) = .{};
    defer to_remove.deinit(alloc);

    var it = runtimes.iterator();
    while (it.next()) |entry| {
        if (entry.value_ptr.*.exited.load(.acquire)) {
            try to_remove.append(alloc, entry.key_ptr.*);
        }
    }

    for (to_remove.items) |key| {
        // Order matters: `key` is the slice the runtimes map stored. Free
        // `threads` first — its lookup uses content equality, so `key`
        // must still be a live pointer when we call fetchRemove. Only
        // then free runtimes (whose fetchRemove returns the same slice
        // we hold in `key`, after which `key` is dangling).
        if (threads.fetchRemove(key)) |kv| {
            kv.value.detach();
            alloc.free(kv.key);
        }
        if (runtimes.fetchRemove(key)) |kv| {
            alloc.free(kv.key);
            alloc.destroy(kv.value);
        }
    }
}
