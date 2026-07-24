//! The ONLY sanctioned outbound HTTP entry for connector code — every vendor
//! call under `handlers/connectors/` routes through `fetch` (grep-gated: no
//! raw `std.http.Client` elsewhere in this subtree). Pin the pooled socket →
//! arm → fetch → disarm, mirroring the runner's control-plane client, and
//! fail CLOSED when the deadline cannot be enforced.
//!
//! One deliberate divergence from the runner client: a pin failure REFUSES
//! the call (`error.VendorUnreachable`) instead of falling through to an
//! unarmed fetch — a connector call either runs armed or is refused; there is
//! no unbounded branch to take. Residual window: the whole connect phase —
//! name resolution, the TCP dial, AND the TLS handshake reads — runs before a
//! pooled handle exists to arm. DNS + dial are OS-bounded; the TLS handshake
//! read is NOT (`std.http.Client.connect` does TCP+TLS atomically, so we
//! cannot arm between them without a connect-phase deadline mechanism that
//! doesn't exist yet). A vendor that completes TCP then stalls the TLS
//! handshake is the one unbounded branch left — tracked as a follow-up; still
//! a strict improvement over the fully-unbounded pre-M108 call. See
//! docs/architecture/connectors.md §Bounded outbound.
//!
//! Every caller shares the ONE process scheduler and owns nothing per call: the
//! generation-guarded `SocketOwner` is a stack local of `fetch`, so concurrent
//! callers cannot clobber each other's deadline the way a shared watchdog
//! instance could. A fire can only ever reach the exact connection generation
//! that armed it, so a pooled descriptor recycled into a later call is safe.

const std = @import("std");
const logging = @import("log");
const call_deadline = @import("call_deadline");
const http_pin = @import("http_pin");
const ec = @import("../../../errors/error_registry.zig");

const log = logging.scoped(.connectors);

/// The process-owned scheduler every connector call arms against. The refusal
/// logging happens here in `fetch`, which knows the provider and call class the
/// scheduler worker never sees.
pub const Scheduler = call_deadline.ProcessScheduler;

// Per-class vendor deadlines — named once, consumed by the call sites.
/// OAuth token exchange (rare browser round-trip; generous).
pub const TOKEN_EXCHANGE_DEADLINE_MS: u31 = 10_000;
/// Outbound answer post (background worker; a failure is retried with backoff).
pub const OUTBOUND_POST_DEADLINE_MS: u31 = 10_000;
/// Best-effort thread re-read at mention ingress — keeps the shipped 1.5 s
/// bound so a slow vendor page read never stalls an ingress worker.
pub const THREAD_READ_DEADLINE_MS: u31 = 1_500;

/// What kind of vendor call is in flight — the `call_class` log field.
pub const CallClass = enum { token_exchange, installation_verify, outbound_post, thread_read };

/// Status + body of a completed vendor call. Caller owns `body`.
pub const Response = struct { status: u16, body: []u8 };

pub const FetchError = error{
    /// The deadline could not be armed (the process scheduler is stopping or
    /// out of identifiers) — the call was refused before any bytes were sent.
    /// Never runs unbounded.
    SchedulerUnavailable,
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

// Class-neutral event name: warnRefused covers all three UZ-CONN-003 refusal
// classes (deadline fired, scheduler unavailable, vendor unreachable), so the
// name must not imply "deadline" — `reason` carries the per-class distinction.
const EV_VENDOR_REFUSED = "connector_vendor_call_refused";
const R_DEADLINE = "deadline";
const R_SCHEDULER_UNAVAILABLE = "scheduler_unavailable";
const R_VENDOR_UNREACHABLE = "vendor_unreachable";

/// Run one deadline-armed vendor call on a fresh per-call client. The process
/// scheduler enforces the bound; on a fire the pinned socket for THIS call's
/// generation is shut down and the call surfaces `DeadlineExceeded`
/// (distinguished from an ordinary transport failure via the owner's flag).
pub fn fetch(alloc: std.mem.Allocator, io: std.Io, sched: *Scheduler, call: Call) FetchError!Response {
    var client: std.http.Client = .{ .allocator = alloc, .io = io };
    defer client.deinit();

    // The control block exists before the connect, and its generation is what a
    // deadline is armed against — never the descriptor number.
    var owner: call_deadline.SocketOwner = .{};
    const generation = owner.beginAttempt();

    // Armed or refused: no pin → no call (the runner client falls through to
    // an unarmed fetch here; connectors must not).
    const handle = http_pin.pinPooledHandle(&client, call.url) orelse {
        warnRefused(call, R_VENDOR_UNREACHABLE);
        return FetchError.VendorUnreachable;
    };
    _ = owner.attachSocket(generation, handle);

    var guard = sched.arm(owner.target(generation), call.deadline_ms) catch {
        warnRefused(call, R_SCHEDULER_UNAVAILABLE);
        return FetchError.SchedulerUnavailable;
    };
    // Retire the generation, then finish the guard. `finish` is the quiescence
    // barrier: once it returns no interrupt callback is running or can start,
    // so `owner` is safe to leave scope. Registered after `client.deinit` so it
    // runs BEFORE it — the socket must not be torn down under a live callback.
    defer {
        owner.endAttempt();
        _ = guard.finish();
    }

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
        // A redirect's new leg dials a fresh connection whose read is OUTSIDE
        // the armed generation, re-opening the unbounded-read window. Connector
        // vendor endpoints are direct; treat a 3xx as the response, never chase
        // it. (The generation guard now makes a mid-redirect fire harmless
        // rather than a recycled-fd kill, but the read would still be unbounded.)
        .redirect_behavior = .unhandled,
        .response_writer = &aw.writer,
    }) catch {
        if (owner.wasInterrupted()) {
            warnRefused(call, R_DEADLINE);
            return FetchError.DeadlineExceeded;
        }
        warnRefused(call, R_VENDOR_UNREACHABLE);
        return FetchError.VendorUnreachable;
    };
    const resp = aw.toOwnedSlice() catch return FetchError.OutOfMemory;
    return .{ .status = @intFromEnum(result.status), .body = resp };
}

/// One warn line for every UZ-CONN-003 refusal class (deadline fired, watchdog
/// unavailable, vendor unreachable) so all three are greppable — the caller
/// maps them to the same 502 and would otherwise leave vendor-down 502s with
/// no server-side log. Never carries URL query or token material.
fn warnRefused(call: Call, reason: []const u8) void {
    log.warn(EV_VENDOR_REFUSED, .{
        .error_code = ec.ERR_CONNECTOR_VENDOR_DEADLINE,
        .reason = reason,
        .provider = call.provider,
        .call_class = @tagName(call.class),
        .deadline_ms = call.deadline_ms,
    });
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

/// A started process scheduler, as the daemon root owns one.
const TestScheduler = struct {
    backend: call_deadline.MonotonicBackend = .{},
    sched: ?Scheduler = null,

    fn start(self: *TestScheduler) !void {
        self.sched = Scheduler.init(testing.allocator, &self.backend);
        try self.sched.?.start();
    }

    fn deinit(self: *TestScheduler) void {
        if (self.sched) |*s| s.deinit();
    }
};

test "bounded_fetch: deadline fires on a stalled vendor and surfaces DeadlineExceeded" {
    const io = common_lib.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});

    var runner: TestScheduler = .{};
    try runner.start();
    defer runner.deinit();

    const t0 = common_lib.clock.nowMillis();
    const err = fetch(testing.allocator, io, &runner.sched.?, .{
        .url = url,
        .method = .POST,
        .payload = "{}",
        .deadline_ms = StallProbe.DEADLINE_MS,
        .provider = "test",
        .class = .outbound_post,
    });
    const elapsed = common_lib.clock.nowMillis() - t0;
    try testing.expectError(FetchError.DeadlineExceeded, err);
    try testing.expect(elapsed < StallProbe.ELAPSED_BOUND_MS);
}

test "bounded_fetch: an unavailable scheduler refuses the call fail-closed (no unbounded run)" {
    const io = common_lib.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});

    // Never started: arming is refused, which is the fail-closed path a
    // stopping process takes.
    var backend: call_deadline.MonotonicBackend = .{};
    var sched = Scheduler.init(testing.allocator, &backend);
    defer sched.deinit();

    const t0 = common_lib.clock.nowMillis();
    const err = fetch(testing.allocator, io, &sched, .{
        .url = url,
        .method = .GET,
        .deadline_ms = TOKEN_EXCHANGE_DEADLINE_MS,
        .provider = "test",
        .class = .token_exchange,
    });
    const elapsed = common_lib.clock.nowMillis() - t0;
    try testing.expectError(FetchError.SchedulerUnavailable, err);
    // Refused BEFORE the request: a stalled vendor cannot have bounded this —
    // only the refusal path returns this fast against a never-answering peer.
    try testing.expect(elapsed < StallProbe.DEADLINE_MS);
}

test "bounded_fetch: unusable URL or dead vendor is refused, never fetched unarmed" {
    const io = common_lib.globalIo();
    var runner: TestScheduler = .{};
    try runner.start();
    defer runner.deinit();
    // Unparseable / hostless URLs pin-fail → VendorUnreachable.
    try testing.expectError(FetchError.VendorUnreachable, fetch(testing.allocator, io, &runner.sched.?, .{
        .url = "not a url",
        .method = .GET,
        .deadline_ms = OUTBOUND_POST_DEADLINE_MS,
        .provider = "test",
        .class = .outbound_post,
    }));
}
