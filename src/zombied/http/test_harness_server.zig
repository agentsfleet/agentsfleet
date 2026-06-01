// Server bring-up plumbing for the test harness — middleware-registry defaults,
// the auth lookup stubs, and the bind-with-retry loop that races httpz's own
// socket bind against the `allocFreePort` TOCTOU. Split out of
// `test_harness.zig` so each file stays under the file-length cap; the core
// harness's `start()` calls `defaultRegistry` + `bringUpServer` here.

const std = @import("std");
const harness_mod = @import("test_harness.zig");
const auth_mw = @import("../auth/middleware/mod.zig");
const http_server = @import("server.zig");
const test_port = @import("test_port.zig");

const TestHarness = harness_mod.TestHarness;
const Config = harness_mod.Config;

pub fn defaultRegistry(h: *TestHarness, cfg: Config) auth_mw.MiddlewareRegistry {
    _ = cfg;
    return .{
        .bearer_or_api_key = .{ .verifier = &h.verifier },
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .tenant_api_key_mw = .{ .host = undefined, .lookup = stubTenantApiKey },
        // SAFETY: stubRunnerLookup ignores host and returns null, so .host is
        // never dereferenced; runner-authed routes 401 in this harness.
        .runner_bearer_mw = .{ .host = undefined, .lookup = stubRunnerLookup },
        .require_role_admin = .{ .required = .admin },
        .require_role_operator = .{ .required = .operator },
        .platform_admin_mw = .{},
        .webhook_hmac_mw = .{ .secret = "" },
    };
}

fn stubTenantApiKey(_: *anyopaque, _: std.mem.Allocator, _: []const u8) anyerror!?auth_mw.tenant_api_key.LookupResult {
    return null;
}

fn stubRunnerLookup(_: *anyopaque, _: std.mem.Allocator, _: []const u8) anyerror!?auth_mw.runner_bearer.LookupResult {
    return null;
}

/// Max bind attempts before `bringUpServer` surfaces the race as a hard error.
const HARNESS_BIND_ATTEMPTS: u8 = 8;

/// Bring the httpz server up on a free port, retrying on a lost bind race.
///
/// httpz binds its own socket inside `listen()` (no fd-passing API), so the
/// sub-millisecond TOCTOU between `allocFreePort`'s probe-close and httpz's
/// bind can lose the port to a sibling harness under a loaded runner — the
/// failure `allocFreePort`'s own header documents. On a lost race the server
/// thread records it (no panic) and we retry on a fresh port. On success
/// `h.server` + `h.thread` are live and owned by the caller's errdefers.
pub fn bringUpServer(h: *TestHarness, alloc: std.mem.Allocator, cfg: Config) !u16 {
    var attempt: u8 = 0;
    while (true) : (attempt += 1) {
        const port = try test_port.allocFreePort();
        h.bind_failed.store(false, .seq_cst);
        h.server = try http_server.Server.init(&h.ctx, &h.registry, .{
            .port = port,
            .threads = 2,
            .workers = 2,
            .max_clients = 64,
        });
        h.thread = try std.Thread.spawn(.{}, serverThread, .{ h.server, &h.bind_failed });
        if (waitForServer(alloc, port, cfg.wait_timeout_ms, &h.bind_failed)) {
            return port;
        } else |err| {
            if (err == error.ServerBindRace) {
                // listen() bind lost the race → the thread already returned.
                // Join it and free the server; do NOT stop() (the accept loop
                // never started; httpz's eventfd shutdown is only valid
                // post-listen). Then retry on a fresh port.
                h.thread.join();
                h.server.deinit();
                if (attempt + 1 < HARNESS_BIND_ATTEMPTS) continue;
                return error.HarnessServerBindFailed;
            }
            // Timed out with a live listener (not a bind race): stop the loop,
            // join, free, and surface — retrying would not help.
            h.server.stop();
            h.thread.join();
            h.server.deinit();
            return err;
        }
    }
}

fn serverThread(srv: *http_server.Server, bind_failed: *std.atomic.Value(bool)) void {
    // A lost bind race (the allocFreePort TOCTOU) is recorded for bringUpServer
    // to retry on a fresh port — it must never panic the test process.
    srv.listen() catch |err| {
        bind_failed.store(true, .seq_cst);
        std.log.warn("harness server listen failed (retrying on a fresh port): {s}", .{@errorName(err)});
    };
}

fn waitForServer(alloc: std.mem.Allocator, port: u16, timeout_ms: u32, bind_failed: *std.atomic.Value(bool)) !void {
    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/healthz", .{port});
    defer alloc.free(url);
    const poll_interval_ms: u32 = 25;
    const max_attempts: u32 = (timeout_ms + poll_interval_ms - 1) / poll_interval_ms; // ceil div
    var i: u32 = 0;
    while (i < max_attempts) : (i += 1) {
        // The server thread lost the bind race → stop waiting so bringUpServer
        // can retry on a fresh port (don't burn the full timeout).
        if (bind_failed.load(.seq_cst)) return error.ServerBindRace;
        var client: std.http.Client = .{ .allocator = alloc };
        defer client.deinit();
        var buf: std.ArrayList(u8) = .{};
        var writer: std.Io.Writer.Allocating = .fromArrayList(alloc, &buf);
        const result = client.fetch(.{
            .location = .{ .url = url },
            .method = .GET,
            .response_writer = &writer.writer,
        }) catch {
            std.Thread.sleep(poll_interval_ms * std.time.ns_per_ms);
            continue;
        };
        const body = writer.toOwnedSlice() catch &.{};
        alloc.free(body);
        if (@intFromEnum(result.status) == 200) return;
        std.Thread.sleep(poll_interval_ms * std.time.ns_per_ms);
    }
    return error.ServerStartTimeout;
}
