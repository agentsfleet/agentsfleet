//! Real-process integration proofs for the runner worker pool (`worker_pool.zig`).
//! These fork ACTUAL children: the pool leases from an in-process control-plane
//! stub, forks the prebuilt `executor_provider_stub` runner (canned result, no
//! LLM) per lease, and reports — so we prove N leases execute CONCURRENTLY on one
//! host and all N reports land, plus a clean drain joins every worker with no
//! leaked thread or child. The fork path is `std.process.spawn` (async-signal-safe
//! post-fork), so forking from this multithreaded daemon is safe by construction;
//! a separate test pins concurrent spawn/reap directly.
//!
//! Gated to the `executor_provider_stub` build (the integration lane): without it
//! there is no canned-result child, so the bodies SkipZigTest. Run via
//! `zig build --build-file build_runner.zig test-integration`.

const std = @import("std");
const builtin = @import("builtin");
const common = @import("common");
const contract = @import("contract");
const build_options = @import("build_options");

const protocol = contract.protocol;
const Config = @import("daemon/config.zig");
const worker_pool = @import("daemon/worker_pool.zig");
const dts = @import("daemon/deadline_test_support.zig");

const WORKER_COUNT: u32 = 4;
/// Hold each lease response briefly so concurrent pollers overlap observably.
const LEASE_HOLD_MS: u64 = 30;
/// Upper bound on the whole pool run; a stuck worker fails fast, never hangs CI.
const RUN_WATCHDOG_MS: u64 = 20_000;

/// In-process control-plane stub: hands out `total` distinct leases (one per
/// `fetchAdd`), then no-work; counts reports and the peak number of workers
/// simultaneously inside the lease handler (the concurrency witness). ONE
/// acceptor thread (concurrent `accept` on a single `Server` is not safe);
/// each accepted connection is handled on its own thread so the brief lease hold
/// overlaps across workers — that overlap is what proves concurrency.
const ControlPlaneStub = struct {
    io: std.Io,
    listener: *std.Io.net.Server,
    total: u32,
    handlers: std.ArrayList(std.Thread) = .empty,
    next_lease: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),
    reports: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),
    heartbeats: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),
    in_lease: std.atomic.Value(i64) = std.atomic.Value(i64).init(0),
    max_in_lease: std.atomic.Value(i64) = std.atomic.Value(i64).init(0),
    stop: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),

    fn acceptLoop(self: *ControlPlaneStub) void {
        while (true) {
            const conn = self.listener.accept(self.io) catch return; // listener closed → exit
            if (self.stop.load(.seq_cst)) { // the shutdown wake connection — drain and exit
                conn.close(self.io);
                return;
            }
            const t = std.Thread.spawn(.{}, handleConn, .{ self, conn }) catch {
                conn.close(self.io);
                continue;
            };
            // On the rare append-OOM, join inline rather than detach: a detached
            // handler holds `*ControlPlaneStub` (stack-allocated in the test) and
            // could outlive the test frame → UB. Inline join is fine on this cold path.
            self.handlers.append(std.heap.page_allocator, t) catch t.join();
        }
    }

    /// Join every handler thread, then free the list. Call AFTER the acceptor has
    /// exited (listener closed), so no new handler is appended concurrently.
    fn joinHandlers(self: *ControlPlaneStub) void {
        for (self.handlers.items) |t| t.join();
        self.handlers.deinit(std.heap.page_allocator);
    }

    fn handleConn(self: *ControlPlaneStub, conn: anytype) void {
        defer conn.close(self.io);
        self.handle(conn);
    }

    fn handle(self: *ControlPlaneStub, conn: anytype) void {
        var buf: [2048]u8 = undefined;
        const line = readRequestLine(conn, &buf) orelse return;
        if (std.mem.indexOf(u8, line, protocol.PATH_RUNNER_HEARTBEATS) != null) {
            _ = self.heartbeats.fetchAdd(1, .seq_cst); // workers must NEVER hit this
            return writeJson(self.io, conn, "{\"status\":\"ok\"}");
        }
        if (std.mem.indexOf(u8, line, protocol.PATH_RUNNER_REPORTS) != null) {
            _ = self.reports.fetchAdd(1, .seq_cst);
            return writeJson(self.io, conn, "{\"ok\":true}");
        }
        if (std.mem.indexOf(u8, line, protocol.PATH_RUNNER_MEMORY) != null)
            return writeJson(self.io, conn, "{\"memory\":[]}");
        if (std.mem.indexOf(u8, line, protocol.PATH_RUNNER_LEASES) != null)
            return self.serveLease(conn);
        writeJson(self.io, conn, "{}"); // renew/activity/capture catch-all
    }

    /// Hand out the next distinct lease (or no-work past `total`), holding the
    /// response `LEASE_HOLD_MS` so concurrent pollers overlap — the peak overlap
    /// is the concurrency proof.
    fn serveLease(self: *ControlPlaneStub, conn: anytype) void {
        const depth = self.in_lease.fetchAdd(1, .seq_cst) + 1;
        _ = self.max_in_lease.fetchMax(depth, .seq_cst);
        common.sleepNanos(LEASE_HOLD_MS * std.time.ns_per_ms);
        defer _ = self.in_lease.fetchSub(1, .seq_cst);

        const idx = self.next_lease.fetchAdd(1, .seq_cst);
        if (idx >= self.total) return writeJson(self.io, conn, "{\"lease\":null,\"retry_after_ms\":50}");

        var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
        defer arena.deinit();
        const json = leaseJson(arena.allocator(), idx) catch return writeJson(self.io, conn, "{\"lease\":null}");
        writeJson(self.io, conn, json);
    }
};

/// Serialize a `LeaseResponse` carrying lease #idx — distinct lease_id / event_id
/// / fleet_id so a double-claim would surface as a duplicate downstream.
fn leaseJson(alloc: std.mem.Allocator, idx: u32) ![]const u8 {
    const payload = protocol.LeasePayload{
        .lease_id = try std.fmt.allocPrint(alloc, "lease-{d}", .{idx}),
        .fencing_token = idx + 1,
        .lease_expires_at = common.clock.nowMillis() + 120_000,
        .secret_delivery = .@"inline",
        .event = .{
            .event_id = try std.fmt.allocPrint(alloc, "1700000000000-{d}", .{idx}),
            .fleet_id = try std.fmt.allocPrint(alloc, "0190aaaa-bbbb-7ccc-8ddd-00000000000{d}", .{idx}),
            .workspace_id = "0190cccc-dddd-7eee-8fff-aaaaaaaaaaaa",
            .actor = "steer:test",
            .event_type = .chat,
            .request_json = "{\"message\":\"hi\"}",
            .created_at = 1700000000000,
        },
        .policy = .{},
    };
    return std.json.Stringify.valueAlloc(alloc, protocol.LeaseResponse{ .lease = payload }, .{});
}

test "worker pool runs N leases concurrently and reports them all, then drains clean" {
    if (!build_options.executor_provider_stub) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    // A real threaded io: `globalIo()` carries a `.failing` allocator, and
    // `std.process.spawn` (the fork path) allocates argv/envp via the io's
    // allocator — so the pool's forks need a real one.
    var threaded = std.Io.Threaded.init(std.heap.page_allocator, .{});
    defer threaded.deinit();
    const io = threaded.io();

    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = try addr.listen(io, .{ .reuse_address = true });
    const port = try boundPort(listener.socket.handle);

    var stub = ControlPlaneStub{ .io = io, .listener = &listener, .total = WORKER_COUNT };
    const acceptor = try std.Thread.spawn(.{}, ControlPlaneStub.acceptLoop, .{&stub});

    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}", .{port});
    defer alloc.free(url);
    const cfg = stubCfg(url);
    // main.zig creates the workspace base; the pool driver bypasses main, so
    // make it here or every prepareWorkspace fails and no lease executes.
    std.Io.Dir.createDirAbsolute(io, cfg.workspace_base, .default_dir) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => return err,
    };

    var env_map: std.process.Environ.Map = .init(alloc);
    defer env_map.deinit();
    var stop = std.atomic.Value(bool).init(false);
    var drain = std.atomic.Value(bool).init(false);

    // The pool borrows the one process scheduler, owned here as main.zig owns it.
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var pool = try worker_pool.spawn(io, alloc, try deadlines.start(alloc), cfg, &env_map, &stop, &drain);

    // Wait for all N reports (the pool executed and reported N leases), bounded.
    var waited: u64 = 0;
    while (stub.reports.load(.seq_cst) < WORKER_COUNT and waited < RUN_WATCHDOG_MS) {
        common.sleepNanos(20 * std.time.ns_per_ms);
        waited += 20;
    }

    drain.store(true, .seq_cst); // graceful: finish in-flight, take no new lease
    try pool.join(); // real workers, no leak → .ok; join surfaces a leak verdict otherwise
    // Deinit'ing the listener does NOT reliably unblock a concurrent accept() on
    // Linux (it does on macOS/BSD), and deinit-during-accept races the acceptor.
    // So: flag stop, wake the blocked accept() with a throwaway self-connection,
    // join the acceptor, then deinit the listener once nothing is in accept().
    stub.stop.store(true, .seq_cst);
    wakeAcceptor(io, port);
    acceptor.join();
    stub.joinHandlers();
    listener.deinit(io);

    try std.testing.expectEqual(WORKER_COUNT, stub.reports.load(.seq_cst)); // all N executed + reported
    try std.testing.expect(stub.max_in_lease.load(.seq_cst) >= 2); // genuinely concurrent, not serialized
    try std.testing.expectEqual(@as(u32, 0), stub.heartbeats.load(.seq_cst)); // heartbeat is the control loop's job — workers never emit one (Invariant 5)
}

test "concurrent forked children spawn and reap from many threads without deadlock" {
    if (builtin.os.tag != .linux and builtin.os.tag != .macos) return error.SkipZigTest;
    const Spawner = struct {
        ok: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),
        io: std.Io,
        fn run(self: *@This()) void {
            const io = self.io;
            const argv = [_][]const u8{ "/bin/sh", "-c", "exit 0" };
            // Same spawn primitive forkExec uses (std.process.spawn = async-signal-safe
            // post-fork), exercised from N threads at once to prove no fork deadlock.
            var child = std.process.spawn(io, .{
                .argv = &argv,
                .stdin = .ignore,
                .stdout = .ignore,
                .stderr = .ignore,
            }) catch return;
            const term = child.wait(io) catch return;
            if (term == .exited and term.exited == 0) _ = self.ok.fetchAdd(1, .seq_cst);
        }
    };
    var threaded = std.Io.Threaded.init(std.heap.page_allocator, .{});
    defer threaded.deinit();
    var spawner = Spawner{ .io = threaded.io() };
    var threads: [8]std.Thread = undefined;
    for (&threads) |*t| t.* = try std.Thread.spawn(.{}, Spawner.run, .{&spawner});
    for (&threads) |*t| t.join();
    // Every concurrent fork/exec/reap from the multithreaded process completed.
    try std.testing.expectEqual(@as(u32, threads.len), spawner.ok.load(.seq_cst));
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Daemon Config for the stub run: dev_none (no bwrap/cgroup), N workers, the
/// stub control plane. Static string fields — no `deinit`.
fn stubCfg(url: []const u8) Config {
    return .{
        .control_plane_url = url,
        .runner_token = contract.protocol.RUNNER_TOKEN_PREFIX ++ "a" ** 8,
        .host_id = "pool-integ-host",
        .sandbox_tier = .dev_none,
        .workspace_base = "/tmp/agentsfleet-runner-pool-integ",
        .network_policy = .deny_all_egress,
        .registry_allowlist = &.{},
        .cp_deadlines = .{},
        .worker_count = WORKER_COUNT,
        .alloc = std.testing.allocator,
    };
}

/// Read the first request line ("METHOD PATH HTTP/1.1") into `buf`; returns a
/// slice of `buf` or null on a closed/empty connection. Body is ignored (we
/// route on method+path and reply `Connection: close`).
fn readRequestLine(conn: anytype, buf: []u8) ?[]const u8 {
    var total: usize = 0;
    while (total < buf.len) {
        const n = std.posix.read(conn.socket.handle, buf[total..]) catch break;
        if (n == 0) break;
        total += n;
        if (std.mem.indexOf(u8, buf[0..total], "\r\n") != null) break;
    }
    const end = std.mem.indexOf(u8, buf[0..total], "\r\n") orelse return null;
    return buf[0..end];
}

/// Write a minimal `200 OK` JSON response and close (one request per connection).
fn writeJson(io: std.Io, conn: anytype, body: []const u8) void {
    var wbuf: [4096]u8 = undefined;
    var w = conn.writer(io, &wbuf);
    w.interface.print(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {d}\r\nConnection: close\r\n\r\n{s}",
        .{ body.len, body },
    ) catch return;
    w.interface.flush() catch return;
}

/// Kernel-assigned local port off a bound listener handle (no getsockname on the
/// 0.16 Server; go through libc on the raw fd — mirrors loop_test).
/// Unblock the acceptor's blocking `accept()` by making one throwaway loopback
/// connection. With `stop` already set, the acceptor accepts this, sees stop,
/// closes it, and returns — portable where `listener.deinit` alone is not.
fn wakeAcceptor(io: std.Io, port: u16) void {
    var addr = std.Io.net.IpAddress.parseIp4("127.0.0.1", port) catch return;
    const stream = addr.connect(io, .{ .mode = .stream }) catch return;
    stream.close(io);
}

fn boundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success; the !=0
    // branch returns an error without reading sa.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}
