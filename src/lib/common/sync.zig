//! Blocking sync primitives bound to one process-global `Io` — Zig 0.16
//! parameterized `std.Io.Mutex`/`Condition` on `Io`; we keep arg-free call sites.

const std = @import("std");

/// Sole site naming the raw std global — repoint here for 0.17+ / a future async runtime.
pub fn globalIo() std.Io {
    return std.Io.Threaded.global_single_threaded.io();
}

/// Best-effort blocking sleep for background timer threads (event bus, OTLP
/// flush, retry backoff). Zig 0.16 removed `std.Thread.sleep`; this routes a
/// timer wait through the same `globalIo()` seam as the blocking sync
/// primitives. Cancellation is swallowed — callers treat the sleep as pacing.
pub fn sleepNanos(ns: u64) void {
    globalIo().sleep(std.Io.Duration.fromNanoseconds(@intCast(ns)), .awake) catch return;
}

/// Blocking mutex, pre-0.16 arg-free `lock`/`unlock` shape; owns no resource (no deinit).
pub const Mutex = struct {
    inner: std.Io.Mutex = .init,

    pub fn lock(self: *Mutex) void {
        self.inner.lockUncancelable(globalIo());
    }

    pub fn unlock(self: *Mutex) void {
        self.inner.unlock(globalIo());
    }

    pub fn tryLock(self: *Mutex) bool {
        return self.inner.tryLock();
    }
};

/// Counting barrier — `std.Thread.WaitGroup` is absent in this Zig, so it is
/// rebuilt on the `Mutex`/`Condition` above. `start` registers a pending unit,
/// `finish` retires one and wakes waiters when the count reaches zero, `wait`
/// blocks until then. Reusable across rounds; the count is guarded so start/
/// finish/wait are safe from any thread (detached workers `finish`, a teardown
/// thread `wait`s).
pub const WaitGroup = struct {
    mutex: Mutex = .{},
    cond: Condition = .{},
    count: usize = 0,

    pub fn start(self: *WaitGroup) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        self.count += 1;
    }

    pub fn finish(self: *WaitGroup) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        self.count -= 1;
        if (self.count == 0) self.cond.broadcast();
    }

    pub fn wait(self: *WaitGroup) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        while (self.count != 0) self.cond.wait(&self.mutex);
    }
};

/// Condition variable paired with `Mutex` (faithful `std.Io.Condition` wrapper).
pub const Condition = struct {
    inner: std.Io.Condition = .init,

    pub fn wait(self: *Condition, mutex: *Mutex) void {
        self.inner.waitUncancelable(globalIo(), &mutex.inner);
    }

    pub fn signal(self: *Condition) void {
        self.inner.signal(globalIo());
    }

    pub fn broadcast(self: *Condition) void {
        self.inner.broadcast(globalIo());
    }
};
