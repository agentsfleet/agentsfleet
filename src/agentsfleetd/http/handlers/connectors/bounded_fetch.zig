//! The ONLY sanctioned outbound HTTP entry for connector code — every vendor
//! call under `handlers/connectors/` routes through `fetch` (grep-gated: no
//! raw `std.http.Client` elsewhere in this subtree). Pin the pooled socket →
//! arm → fetch → disarm, mirroring the runner's control-plane client, and
//! fail CLOSED when the deadline cannot be enforced.
//!
//! One deliberate divergence from the runner client: a pin failure REFUSES
//! the call (`error.VendorUnreachable`) instead of falling through to an
//! unarmed fetch — a connector call either runs armed or is refused; there is
//! no unbounded branch to take. Residual window: name resolution + the TCP
//! dial are connect-phase semantics (OS-bounded), not the read watchdog —
//! see docs/architecture/connectors.md.
//!
//! Callers own watchdog lifetime by context: the outbound worker keeps one
//! across its loop; the request-scoped paths (OAuth exchange, thread re-read)
//! hold one per request — a watchdog arms exactly ONE call at a time, so a
//! shared instance on a concurrent path would let two arms clobber each other.

const std = @import("std");
const logging = @import("log");
const call_deadline = @import("call_deadline");
const ec = @import("../../../errors/error_registry.zig");

const log = logging.scoped(.connectors);

/// Silent mechanism (`Watchdog(null)`): the fire/refusal logging happens here
/// in `fetch`, which knows the provider + call class the watchdog thread never
/// sees.
pub const Watchdog = call_deadline.Watchdog(null);

// Per-class vendor deadlines — named once, consumed by the call sites.
/// OAuth token exchange (rare browser round-trip; generous).
pub const TOKEN_EXCHANGE_DEADLINE_MS: u31 = 10_000;
/// Outbound answer post (background worker; a failure is retried with backoff).
pub const OUTBOUND_POST_DEADLINE_MS: u31 = 10_000;
/// Best-effort thread re-read at mention ingress — keeps the shipped 1.5 s
/// bound so a slow vendor page read never stalls an ingress worker.
pub const THREAD_READ_DEADLINE_MS: u31 = 1_500;

/// What kind of vendor call is in flight — the `call_class` log field.
pub const CallClass = enum { token_exchange, outbound_post, thread_read };

/// Status + body of a completed vendor call. Caller owns `body`.
pub const Response = struct { status: u16, body: []u8 };

pub const FetchError = error{
    /// The deadline could not be armed (watchdog thread unavailable) — the
    /// call was refused before any bytes were sent. Never runs unbounded.
    WatchdogUnavailable,
    /// The armed deadline fired mid-call (vendor accepted, then stalled).
    DeadlineExceeded,
    /// Dial/pin/transport failure without a deadline fire (vendor down, DNS,
    /// connection reset).
    VendorUnreachable,
    OutOfMemory,
};

/// One vendor call. `provider` + `class` ride the deadline log, never the URL
/// (no query/token material in log fields).
pub const Call = struct {
    url: []const u8,
    method: std.http.Method,
    payload: ?[]const u8 = null,
    extra_headers: []const std.http.Header = &.{},
    deadline_ms: u31,
    provider: []const u8,
    class: CallClass,
};

const EV_VENDOR_DEADLINE = "connector_vendor_deadline_fired";
const R_DEADLINE = "deadline";
const R_WATCHDOG_UNAVAILABLE = "watchdog_unavailable";

/// Run one deadline-armed vendor call on a fresh per-call client. The caller's
/// watchdog enforces the bound; on a fire the pooled socket is shut down and
/// the call surfaces `DeadlineExceeded` (distinguished from an ordinary
/// transport failure via the watchdog's fired flag).
pub fn fetch(alloc: std.mem.Allocator, io: std.Io, wd: *Watchdog, call: Call) FetchError!Response {
    var client: std.http.Client = .{ .allocator = alloc, .io = io };
    defer client.deinit();

    // Armed or refused: no pin → no call (the runner client falls through to
    // an unarmed fetch here; connectors must not).
    const handle = pinPooledHandle(&client, call.url) orelse return FetchError.VendorUnreachable;
    if (wd.arm(handle, call.deadline_ms) == .watchdog_unavailable) {
        warnDeadline(call, R_WATCHDOG_UNAVAILABLE);
        return FetchError.WatchdogUnavailable;
    }
    defer wd.disarm();

    // BUFFER GATE: Allocating writer over an ArrayList(u8) — append-as-you-go,
    // size unknown until the vendor responds; read once via toOwnedSlice.
    var body: std.ArrayList(u8) = .empty;
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &body);
    errdefer aw.deinit();

    const result = client.fetch(.{
        .location = .{ .url = call.url },
        .method = call.method,
        .payload = call.payload,
        .extra_headers = call.extra_headers,
        .response_writer = &aw.writer,
    }) catch {
        if (wd.deadlineFired()) {
            warnDeadline(call, R_DEADLINE);
            return FetchError.DeadlineExceeded;
        }
        return FetchError.VendorUnreachable;
    };
    const resp = aw.toOwnedSlice() catch return FetchError.OutOfMemory;
    return .{ .status = @intFromEnum(result.status), .body = resp };
}

fn warnDeadline(call: Call, reason: []const u8) void {
    log.warn(EV_VENDOR_DEADLINE, .{
        .error_code = ec.ERR_CONNECTOR_VENDOR_DEADLINE,
        .reason = reason,
        .provider = call.provider,
        .call_class = @tagName(call.class),
        .deadline_ms = call.deadline_ms,
    });
}

/// Pin the pooled connection the fetch will use (get-or-create, then release
/// back to the free list so the fetch pops the same one) and return its socket
/// handle for the watchdog. Null when the URL is unusable or the dial fails —
/// the call is refused.
fn pinPooledHandle(client: *std.http.Client, url: []const u8) ?std.Io.net.Socket.Handle {
    const uri = std.Uri.parse(url) catch return null;
    const tls = std.ascii.eqlIgnoreCase(uri.scheme, "https");
    const port: u16 = uri.port orelse @as(u16, if (tls) 443 else 80);
    const raw_host = uri.host orelse return null;
    const host_str = switch (raw_host) {
        .raw => |r| r,
        .percent_encoded => |p| p,
    };
    if (host_str.len == 0) return null;
    const host = std.Io.net.HostName.init(host_str) catch return null;
    const conn = client.connect(host, port, if (tls) .tls else .plain) catch return null;
    const handle = conn.stream_writer.stream.socket.handle;
    client.connection_pool.release(conn, client.io);
    return handle;
}

// ── Tests (real loopback sockets; no DB — the deadline mechanism is pure) ────

const testing = std.testing;
const common_lib = @import("common");

/// Ephemeral port of a bound listener (port 0 → OS-assigned; read it back).
fn boundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success; the !=0
    // branch returns an error without reading sa.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

/// A vendor that never answers: listening without accept(2) completes the TCP
/// handshake via the backlog, so the pin + send succeed and the read stalls —
/// the exact hung-vendor shape the watchdog exists for.
const StallProbe = struct {
    /// Well over the test deadline; well under the suite timeout — proves the
    /// return came from the fired deadline, not the vendor.
    const DEADLINE_MS: u31 = 250;
    const ELAPSED_BOUND_MS: i64 = 2_000;
};

test "bounded_fetch: deadline fires on a stalled vendor and surfaces DeadlineExceeded" {
    const io = common_lib.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});

    var wd: Watchdog = .{};
    defer wd.deinit();

    const t0 = common_lib.clock.nowMillis();
    const err = fetch(testing.allocator, io, &wd, .{
        .url = url,
        .method = .POST,
        .payload = "{}",
        .deadline_ms = StallProbe.DEADLINE_MS,
        .provider = "test",
        .class = .outbound_post,
    });
    const elapsed = common_lib.clock.nowMillis() - t0;
    try testing.expectError(FetchError.DeadlineExceeded, err);
    try testing.expect(wd.deadlineFired());
    try testing.expect(elapsed < StallProbe.ELAPSED_BOUND_MS);
}

test "bounded_fetch: watchdog unavailable refuses the call fail-closed (no unbounded run)" {
    const io = common_lib.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});

    var wd: Watchdog = .{ .force_spawn_fail_for_test = true };
    defer wd.deinit();

    const t0 = common_lib.clock.nowMillis();
    const err = fetch(testing.allocator, io, &wd, .{
        .url = url,
        .method = .GET,
        .deadline_ms = TOKEN_EXCHANGE_DEADLINE_MS,
        .provider = "test",
        .class = .token_exchange,
    });
    const elapsed = common_lib.clock.nowMillis() - t0;
    try testing.expectError(FetchError.WatchdogUnavailable, err);
    // Refused BEFORE the request: a stalled vendor cannot have bounded this —
    // only the refusal path returns this fast against a never-answering peer.
    try testing.expect(elapsed < StallProbe.DEADLINE_MS);
}

test "bounded_fetch: unusable URL or dead vendor is refused, never fetched unarmed" {
    const io = common_lib.globalIo();
    var wd: Watchdog = .{};
    defer wd.deinit();
    // Unparseable / hostless URLs pin-fail → VendorUnreachable.
    try testing.expectError(FetchError.VendorUnreachable, fetch(testing.allocator, io, &wd, .{
        .url = "not a url",
        .method = .GET,
        .deadline_ms = OUTBOUND_POST_DEADLINE_MS,
        .provider = "test",
        .class = .outbound_post,
    }));
    try testing.expect(!wd.deadlineFired());
}
