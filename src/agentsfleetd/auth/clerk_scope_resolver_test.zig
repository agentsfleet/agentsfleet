//! Tests for the resolver's public surface — the type-erased adapter the
//! middleware actually holds. Cache policy is proved in-file next door, where
//! the cache is visible; what matters here is that the middleware's call shape
//! reaches the same behaviour, because the middleware only ever sees this
//! function pointer and an opaque host.

const std = @import("std");
const testing = std.testing;
const common = @import("common");

const resolver_mod = @import("clerk_scope_resolver.zig");
const scopes_mod = @import("scopes.zig");

const SUBJECT = "user_2aXyTest";

test "the exported adapter satisfies the middleware's injected scope callback" {
    // A compile-time proof, not a formality: the middleware takes this by
    // function pointer, so a signature drift would otherwise surface only when
    // the boot host is wired — long after this module looked finished.
    const injected: scopes_mod.ScopeFn = resolver_mod.resolveScopes;
    try testing.expect(injected == resolver_mod.resolveScopes);
}

test "a resolve through the opaque host reaches the same refusal as a direct call" {
    var resolver = resolver_mod.ScopeResolver.init(testing.allocator, .{ .secret = null });
    defer resolver.deinit();

    // No provider secret and nothing cached: the caller is told authentication
    // is unavailable, which the middleware turns into its own registered code.
    try testing.expectError(
        resolver_mod.ResolveError.ScopesUnavailable,
        resolver_mod.resolveScopes(&resolver, testing.allocator, SUBJECT),
    );
}

test "the freshness window and the stale ceiling carry usable defaults" {
    // The window is short enough that a dashboard revocation reaches a
    // terminal on roughly the cadence the dashboard itself refreshes, and the
    // ceiling is far above it so a blip is ridden out rather than fatal.
    var resolver = resolver_mod.ScopeResolver.init(testing.allocator, .{ .secret = null });
    defer resolver.deinit();

    try testing.expectEqual(resolver_mod.DEFAULT_TTL_MS, resolver.ttl_ms);
    try testing.expectEqual(resolver_mod.DEFAULT_STALE_CEILING_MS, resolver.stale_ceiling_ms);
    try testing.expect(resolver_mod.DEFAULT_STALE_CEILING_MS > resolver_mod.DEFAULT_TTL_MS);
}

// ── Concurrency: the shared-read lock under real contention ──────────────────
// The resolver serves BOTH resolved credential classes per request, so cache
// hits take a shared lock and dupe the entry inside it. These constants bound
// the race: enough threads to contend, enough iterations that expiry/store
// interleaves with reads.
const CONC_THREADS: usize = 8;
const CONC_RESOLVES_PER_THREAD: usize = 16;
const CONC_CLAIM = "fleet:read model:read";
const CONC_BODY =
    \\{"public_metadata":{"scopes":"fleet:read model:read"}}
;
const CONC_SECRET = "resolver-conc-fixture";

fn concBoundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

const CONC_READ_TIMEOUT_MS: u32 = 1_000;
const CONC_MS_PER_SECOND: u32 = 1000;
const CONC_US_PER_MS: u32 = 1000;

fn concSetReadTimeout(fd: std.posix.fd_t, ms: u32) void {
    const timeout = std.posix.timeval{
        .sec = @intCast(ms / CONC_MS_PER_SECOND),
        .usec = @intCast((ms % CONC_MS_PER_SECOND) * CONC_US_PER_MS),
    };
    std.posix.setsockopt(fd, std.posix.SOL.SOCKET, std.posix.SO.RCVTIMEO, std.mem.asBytes(&timeout)) catch |err|
        std.log.warn("conc stub read-timeout not set (best effort): {s}", .{@errorName(err)});
}

const ConcStub = struct {
    // Serves every connection the same 200 user document until stopped; the
    // resolver is deliberately not single-flighted, so the number of fetches
    // under contention is a race outcome, not a constant.
    fn serve(listener: *std.Io.net.Server, io: std.Io, stop: *std.atomic.Value(bool), response: []const u8) void {
        while (true) {
            const conn = listener.accept(io) catch return;
            defer conn.close(io);
            // safe because: shutdown's release-store pairs with this acquire-load.
            if (stop.load(.acquire)) return;
            // Bounded read: a dead peer must not park this thread and wedge
            // the join; a zero read means peer closed — writing would SIGPIPE.
            concSetReadTimeout(conn.socket.handle, CONC_READ_TIMEOUT_MS);
            var rbuf: [2048]u8 = undefined;
            const n = std.posix.read(conn.socket.handle, &rbuf) catch continue;
            if (n == 0) continue;
            var sent: usize = 0;
            while (sent < response.len) {
                const rc = std.posix.system.write(conn.socket.handle, response[sent..].ptr, response.len - sent);
                if (std.posix.errno(rc) != .SUCCESS) break;
                sent += @intCast(rc);
            }
        }
    }

    fn resolveMany(resolver: *resolver_mod.ScopeResolver, start: *std.atomic.Value(bool), failed: *std.atomic.Value(bool)) void {
        // safe because: the spawner's release-store pairs with this acquire-load.
        while (!start.load(.acquire)) common.sleepNanos(std.time.ns_per_us);
        var i: usize = 0;
        while (i < CONC_RESOLVES_PER_THREAD) : (i += 1) {
            const claim = resolver.resolve(testing.allocator, SUBJECT) catch {
                failed.store(true, .release);
                return;
            };
            defer testing.allocator.free(claim);
            if (!std.mem.eql(u8, claim, CONC_CLAIM)) {
                failed.store(true, .release);
                return;
            }
        }
    }
};

test "should serve every concurrent resolve the provider's claim under the shared lock" {
    // Catches: a dupe moved outside the shared section (use-after-free once a
    // concurrent store swaps the entry), reader-vs-writer corruption after the
    // Mutex→RwLock change, and any leak on the contended paths — the
    // leak-detecting allocator is the arbiter for all three.
    const io = common.globalIo();
    var addr = std.Io.net.IpAddress.parseIp4("127.0.0.1", 0) catch return error.SkipZigTest;
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    const port = concBoundPort(listener.socket.handle) catch {
        listener.deinit(io);
        return error.SkipZigTest;
    };

    const response = try std.fmt.allocPrint(
        testing.allocator,
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {d}\r\nconnection: close\r\n\r\n{s}",
        .{ CONC_BODY.len, CONC_BODY },
    );
    defer testing.allocator.free(response);

    var stop = std.atomic.Value(bool).init(false);
    const stub = try std.Thread.spawn(.{}, ConcStub.serve, .{ &listener, io, &stop, response });

    var base_buf: [44]u8 = undefined;
    const api_base = try std.fmt.bufPrint(&base_buf, "http://127.0.0.1:{d}", .{port});
    // A one-millisecond freshness window forces expiry mid-run, so reads,
    // stale copies, and stores genuinely interleave rather than the first
    // fetch settling everything.
    var resolver = resolver_mod.ScopeResolver.init(testing.allocator, .{
        .secret = CONC_SECRET,
        .api_base = api_base,
        .ttl_ms = 1,
    });
    defer resolver.deinit();

    var start = std.atomic.Value(bool).init(false);
    var failed = std.atomic.Value(bool).init(false);
    var threads: [CONC_THREADS]std.Thread = undefined;
    var spawned: usize = 0;
    for (&threads) |*t| {
        t.* = std.Thread.spawn(.{}, ConcStub.resolveMany, .{ &resolver, &start, &failed }) catch break;
        spawned += 1;
    }
    // Released even on a partial spawn so already-started workers never spin
    // forever on a start flag nobody will set (spawn-failure teardown path).
    // safe because: paired with the acquire-load in resolveMany.
    start.store(true, .release);
    for (threads[0..spawned]) |t| t.join();
    try testing.expectEqual(@as(usize, CONC_THREADS), spawned);

    // Stop the stub: set stop, wake the blocked accept with one poke connect,
    // join, and only then deinit the listener (Linux accept does not wake on
    // deinit alone).
    stop.store(true, .release);
    // A failed wake would leave join() blocked on accept() forever; three
    // attempts then a loud panic beats a silently wedged test binary.
    var woken = false;
    var attempt: usize = 0;
    while (attempt < 3 and !woken) : (attempt += 1) {
        var wake_addr = std.Io.net.IpAddress.parseIp4("127.0.0.1", port) catch continue;
        const s = wake_addr.connect(io, .{ .mode = .stream }) catch continue;
        s.close(io);
        woken = true;
    }
    if (!woken) @panic("conc stub wake failed — acceptor cannot be joined");
    stub.join();
    listener.deinit(io);

    try testing.expect(!failed.load(.acquire));
}
