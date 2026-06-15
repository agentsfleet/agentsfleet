//! HTTP client the host daemon uses to drive the `/v1/runners/me/*` control
//! plane. It POSTs lease/heartbeat/report/activity to a `agentsfleetd` instance
//! using the same frozen `protocol` shapes the server speaks, so server and
//! client cannot drift. Enrollment is not a daemon concern (Option B): the
//! operator pre-mints the `zrn_` and the daemon authenticates with it directly.
//!
//! Uses the high-level `std.http.Client.fetch` (cross-platform; the manual
//! `open()`/`readVec()` path is Linux-broken under Zig 0.15) over ONE
//! persistent `std.http.Client` per owner (keep-alive connection reuse —
//! a chatty agent run no longer pays a TCP/TLS handshake per frame).
//!
//! Every verb takes a required `deadline_ms`: a per-client watchdog
//! (call_deadline.zig) shuts the in-flight pooled socket down at the bound,
//! so a hung control plane surfaces as a retryable transport error instead of
//! wedging the worker (and starving the child's deadline kill). Residual
//! window: name resolution + TCP connect inside fetch are not armed (the
//! production control plane is loopback/intra-region).

const LoopbackClient = @This();

/// Base origin of the control plane, e.g. `http://127.0.0.1:8080` (no path).
base_url: []const u8,
/// Blocking `Io` the outbound `std.http.Client` runs on (Zig 0.16 requires it as
/// a no-default field). Borrowed from the daemon's `Io.Threaded`; the client
/// never owns or deinits it — lifetime is the process.
io: std.Io,
/// Persistent HTTP client (connection pool). Owned: deinit() closes it.
http: std.http.Client,
/// Parsed once from base_url for the pre-fetch connection pinning.
host: []const u8,
port: u16,
tls: bool,
/// Bounds the in-flight call (lazy thread; joined by deinit).
watchdog: call_deadline.CallWatchdog = .{},

pub const ClientError = error{ RequestFailed, BadStatus, MalformedResponse };

// Re-exported call-bounding policy (defaults + resolved set) — single source
// in call_deadline.zig; config.zig and the cmd verbs consume these names.
pub const DEFAULT_DEADLINE_MS = call_deadline.DEFAULT_DEADLINE_MS;
pub const REPORT_DEADLINE_MS = call_deadline.REPORT_DEADLINE_MS;
pub const ACTIVITY_DEADLINE_MS = call_deadline.ACTIVITY_DEADLINE_MS;
pub const RENEW_DEADLINE_MS = call_deadline.RENEW_DEADLINE_MS;
pub const Deadlines = call_deadline.Deadlines;

/// Build a client with a persistent connection pool. `alloc` must outlive the
/// client (per-worker allocator); call `deinit()` to close pooled connections.
pub fn init(alloc: Allocator, io: std.Io, base_url: []const u8) LoopbackClient {
    var host: []const u8 = "";
    var port: u16 = 80;
    var tls = false;
    if (std.Uri.parse(base_url)) |uri| {
        tls = std.ascii.eqlIgnoreCase(uri.scheme, "https");
        port = uri.port orelse @as(u16, if (tls) 443 else 80);
        if (uri.host) |h| host = switch (h) {
            .raw => |r| r,
            .percent_encoded => |p| p,
        };
    } else |_| {}
    return .{
        .base_url = base_url,
        .io = io,
        .http = .{ .allocator = alloc, .io = io },
        .host = host,
        .port = port,
        .tls = tls,
    };
}

pub fn deinit(self: *LoopbackClient) void {
    self.watchdog.deinit();
    self.http.deinit();
}

/// POST /v1/runners/me/leases → the next event + resolved policy, or no-work.
/// The whole tree (event envelope + secrets_map + budget) lives in the returned
/// arena; the caller deinits after executing and reporting. `.alloc_always` so
/// every string is copied into that arena — otherwise unescaped fields reference
/// `res.body`, which is freed here, leaving the returned `LeasePayload` dangling
/// (a use-after-free the worker pool surfaces when its allocator reuses the
/// buffer). Matches `getSelf`/`memoryHydrate`, which copy for the same reason.
pub fn lease(self: *LoopbackClient, alloc: Allocator, runner_token: []const u8, deadline_ms: u31) !std.json.Parsed(protocol.LeaseResponse) {
    const res = try self.post(alloc, protocol.PATH_RUNNER_LEASES, runner_token, "", deadline_ms);
    defer alloc.free(res.body);
    if (res.status < 200 or res.status >= 300) return ClientError.BadStatus;
    return std.json.parseFromSlice(protocol.LeaseResponse, alloc, res.body, .{ .allocate = .alloc_always }) catch
        ClientError.MalformedResponse;
}

/// POST /v1/runners/me/heartbeats → signal liveness + receive fleet directives.
/// Request body is empty in S0 (capacity/version fields are a later workstream).
/// Returns the parsed HeartbeatResponse so the daemon can act on status==drain/stop.
pub fn heartbeat(self: *LoopbackClient, alloc: Allocator, runner_token: []const u8, deadline_ms: u31) !protocol.HeartbeatResponse {
    const res = try self.post(alloc, protocol.PATH_RUNNER_HEARTBEATS, runner_token, "", deadline_ms);
    defer alloc.free(res.body);
    if (res.status < 200 or res.status >= 300) return ClientError.BadStatus;
    const parsed = std.json.parseFromSlice(protocol.HeartbeatResponse, alloc, res.body, .{}) catch
        return ClientError.MalformedResponse;
    defer parsed.deinit();
    return parsed.value;
}

/// GET /v1/runners/me → the runner's own row, read-only (no liveness bump). The
/// caller deinits the parsed value. `.alloc_always`: the response strings (id,
/// status, host_id, sandbox_tier) must outlive `res.body`, which is freed here.
pub fn getSelf(self: *LoopbackClient, alloc: Allocator, runner_token: []const u8, deadline_ms: u31) !std.json.Parsed(protocol.SelfResponse) {
    const res = try self.get(alloc, protocol.PATH_RUNNER_SELF, runner_token, deadline_ms);
    defer alloc.free(res.body);
    if (res.status < 200 or res.status >= 300) return ClientError.BadStatus;
    return std.json.parseFromSlice(protocol.SelfResponse, alloc, res.body, .{ .allocate = .alloc_always }) catch
        ClientError.MalformedResponse;
}

/// POST /v1/runners/me/reports → finalize one execution. Body is `{ok:true}`;
/// only the 2xx status matters to the caller.
pub fn report(self: *LoopbackClient, alloc: Allocator, runner_token: []const u8, req: protocol.ReportRequest, deadline_ms: u31) !void {
    const payload = try std.json.Stringify.valueAlloc(alloc, req, .{});
    defer alloc.free(payload);
    const res = try self.post(alloc, protocol.PATH_RUNNER_REPORTS, runner_token, payload, deadline_ms);
    defer alloc.free(res.body);
    if (res.status < 200 or res.status >= 300) return ClientError.BadStatus;
}

/// GET /v1/runners/me/memory/{agent_id} → the agent's prior memory (a
/// compacted recency window). The parent seeds the child's in-run store from
/// this; the sandboxed child never makes the call. `.alloc_always` so the
/// returned deltas outlive `res.body` (freed here) — they ride the child input.
/// Caller deinits the parsed value after the run.
pub fn memoryHydrate(self: *LoopbackClient, alloc: Allocator, runner_token: []const u8, agent_id: []const u8, deadline_ms: u31) !std.json.Parsed(protocol.MemoryHydrateResponse) {
    const path = try std.fmt.allocPrint(alloc, "{s}/{s}", .{ protocol.PATH_RUNNER_MEMORY, agent_id });
    defer alloc.free(path);
    const res = try self.get(alloc, path, runner_token, deadline_ms);
    defer alloc.free(res.body);
    if (res.status < 200 or res.status >= 300) return ClientError.BadStatus;
    return std.json.parseFromSlice(protocol.MemoryHydrateResponse, alloc, res.body, .{ .allocate = .alloc_always }) catch
        ClientError.MalformedResponse;
}

/// POST /v1/runners/me/memory/{agent_id} → capture the run's memory for the
/// agent. `lease_id` + `fencing_token` ride the body (like `report`) so the
/// control plane fences the write. Only the 2xx status matters to the caller;
/// the daemon swallows + logs a failure (a memory blip never fails the run).
pub fn memoryCapture(self: *LoopbackClient, alloc: Allocator, runner_token: []const u8, agent_id: []const u8, req: protocol.MemoryPushRequest, deadline_ms: u31) !void {
    const path = try std.fmt.allocPrint(alloc, "{s}/{s}", .{ protocol.PATH_RUNNER_MEMORY, agent_id });
    defer alloc.free(path);
    const payload = try std.json.Stringify.valueAlloc(alloc, req, .{});
    defer alloc.free(payload);
    const res = try self.post(alloc, path, runner_token, payload, deadline_ms);
    defer alloc.free(res.body);
    if (res.status < 200 or res.status >= 300) return ClientError.BadStatus;
}

/// POST /v1/runners/me/leases/{lease_id}/activity → forward live-tail progress
/// frames. `lease_id` is a path param (the only runner verb that takes one).
/// Best-effort by contract (202, no ack): the durable record is `report`, so a
/// failed forward is swallowed and never disturbs execution — hence `void`, not
/// `!void`. Allocation/transport failures drop the frame silently.
const ACTIVITY_BODY_FMT = "{{\"frames\":[{s}]}}";

/// Like `activity`, but the caller supplies the frames as already-serialized
/// JSON objects (comma-joined, no brackets) — the batching forwarder serializes
/// frames on arrival because their slices are only valid during the callback.
pub fn activityFramesJson(
    self: *LoopbackClient,
    alloc: Allocator,
    runner_token: []const u8,
    lease_id: []const u8,
    frames_json: []const u8,
    deadline_ms: u31,
) void {
    const payload = std.fmt.allocPrint(alloc, ACTIVITY_BODY_FMT, .{frames_json}) catch return;
    defer alloc.free(payload);
    self.activityBody(alloc, runner_token, lease_id, payload, deadline_ms);
}

fn activityBody(
    self: *LoopbackClient,
    alloc: Allocator,
    runner_token: []const u8,
    lease_id: []const u8,
    payload: []const u8,
    deadline_ms: u31,
) void {
    const path = std.fmt.allocPrint(alloc, "{s}/{s}/{s}", .{
        protocol.PATH_RUNNER_LEASES, lease_id, protocol.RUNNER_LEASE_ACTIVITY_SUFFIX,
    }) catch return;
    defer alloc.free(path);
    const res = self.post(alloc, path, runner_token, payload, deadline_ms) catch return;
    alloc.free(res.body); // 202 expected; status ignored (best-effort, no ack).
}

/// Outcome of a renewal attempt the caller can act on without re-parsing.
pub const RenewResult = union(enum) {
    /// 2xx — the authoritative new kill deadline (epoch ms). Retarget the child.
    renewed: i64,
    /// A definitive 4xx (lease lost / max-runtime / no-credits): stop renewing
    /// and kill the child. Carries the status for the caller's log; the specific
    /// `UZ-RUN-010/011/012` distinction is server-logged.
    terminal: u16,
};

/// POST /v1/runners/me/leases/{lease_id}/renew → extend the lease's kill
/// deadline while the child is actively executing. `lease_id` is a path param;
/// the body carries the run's cumulative token splits so the control plane
/// charges the diff since its last-metered cursor on every renewal.
///
/// Fail-safe by design: a 2xx yields `renewed`; a definitive 4xx yields
/// `terminal` (the caller kills its child); a transport failure, 5xx, or body
/// serialization failure returns an error so the caller simply retries on the
/// next tick — if renewal keeps failing the lease just expires naturally and
/// is reclaimed (never double-run), and a charge is never invented from a
/// half-built body.
pub fn renew(
    self: *LoopbackClient,
    alloc: Allocator,
    runner_token: []const u8,
    lease_id: []const u8,
    req: protocol.RenewRequest,
    deadline_ms: u31,
) !RenewResult {
    const path = try std.fmt.allocPrint(alloc, "{s}/{s}/{s}", .{
        protocol.PATH_RUNNER_LEASES, lease_id, protocol.RUNNER_LEASE_RENEW_SUFFIX,
    });
    defer alloc.free(path);
    const body = try std.json.Stringify.valueAlloc(alloc, req, .{});
    defer alloc.free(body);
    const res = try self.post(alloc, path, runner_token, body, deadline_ms);
    defer alloc.free(res.body);
    return classifyRenew(alloc, res.status, res.body);
}

/// Map a `/renew` HTTP response (status + body) to a `RenewResult` — the pure,
/// I/O-free core of `renew`, so the status→outcome contract is unit-testable
/// without a live server. A 2xx parses the new kill deadline; a definitive 4xx
/// yields `.terminal`; every other status (other 4xx, 5xx) is `BadStatus` so the
/// caller retries on the next tick.
pub fn classifyRenew(alloc: Allocator, status: u16, body: []const u8) ClientError!RenewResult {
    if (status >= 200 and status < 300) {
        const parsed = std.json.parseFromSlice(protocol.RenewResponse, alloc, body, .{}) catch
            return ClientError.MalformedResponse;
        defer parsed.deinit();
        return .{ .renewed = parsed.value.lease_expires_at };
    }
    if (isTerminalRenewStatus(status)) return .{ .terminal = status };
    return ClientError.BadStatus; // other 4xx (400/429/…) + 5xx → retryable; caller retries next tick.
}

/// Definitive `/renew` rejections the runner must NOT retry (kill the child):
/// 401 invalid/revoked token (UZ-RUN-001), 402 credit exhausted (UZ-RUN-012),
/// 404 lease not found (UZ-RUN-006), 409 lease lost / max-runtime (UZ-RUN-010/011).
/// Any other 4xx (400 body, 429 rate-limit, …) is retryable like a 5xx — a
/// transient/non-terminal status must never kill a healthy in-flight run.
pub fn isTerminalRenewStatus(status: u16) bool {
    return status == 401 or status == 402 or status == 404 or status == 409;
}

const PostResult = struct { status: u16, body: []u8 };

/// Pin the pooled connection the next fetch will use (get-or-create, then
/// release back to the free list so the fetch pops the same one) and return
/// its socket handle for the watchdog. Null when the connect fails — the
/// fetch then fails fast on its own connect attempt.
fn pooledHandle(self: *LoopbackClient) ?std.Io.net.Socket.Handle {
    if (self.host.len == 0) return null;
    const host = std.Io.net.HostName.init(self.host) catch return null;
    const conn = self.http.connect(host, self.port, if (self.tls) .tls else .plain) catch return null;
    const handle = conn.stream_writer.stream.socket.handle;
    self.http.connection_pool.release(conn, self.io);
    return handle;
}

/// One bearer-authed POST on the persistent client. Returns the status +
/// response body (owned by `alloc`).
fn post(self: *LoopbackClient, alloc: Allocator, path: []const u8, bearer: []const u8, payload: []const u8, deadline_ms: u31) !PostResult {
    if (self.pooledHandle()) |handle| self.watchdog.arm(handle, deadline_ms);
    defer self.watchdog.disarm();

    const url = try std.fmt.allocPrint(alloc, "{s}{s}", .{ self.base_url, path });
    defer alloc.free(url);
    const auth = try std.fmt.allocPrint(alloc, "Bearer {s}", .{bearer});
    defer alloc.free(auth);

    // BUFFER GATE: ArrayList for response body — fetch appends as it streams;
    // .items is read once for the JSON parse.
    var body: std.ArrayList(u8) = .empty;
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &body);

    const headers: [2]std.http.Header = .{
        .{ .name = "content-type", .value = "application/json" },
        .{ .name = "authorization", .value = auth },
    };
    const result = self.http.fetch(.{
        .location = .{ .url = url },
        .method = .POST,
        .payload = payload,
        .extra_headers = &headers,
        .response_writer = &aw.writer,
    }) catch return ClientError.RequestFailed;

    return .{ .status = @intFromEnum(result.status), .body = aw.toOwnedSlice() catch return ClientError.RequestFailed };
}

/// One bearer-authed GET (no body) on the persistent client. Returns the status
/// + response body (owned by `alloc`). Used by the read-only `getSelf` verb.
fn get(self: *LoopbackClient, alloc: Allocator, path: []const u8, bearer: []const u8, deadline_ms: u31) !PostResult {
    if (self.pooledHandle()) |handle| self.watchdog.arm(handle, deadline_ms);
    defer self.watchdog.disarm();

    const url = try std.fmt.allocPrint(alloc, "{s}{s}", .{ self.base_url, path });
    defer alloc.free(url);
    const auth = try std.fmt.allocPrint(alloc, "Bearer {s}", .{bearer});
    defer alloc.free(auth);

    // BUFFER GATE: ArrayList for response body — fetch appends as it streams.
    var body: std.ArrayList(u8) = .empty;
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &body);

    const headers: [1]std.http.Header = .{.{ .name = "authorization", .value = auth }};
    const result = self.http.fetch(.{
        .location = .{ .url = url },
        .method = .GET,
        .extra_headers = &headers,
        .response_writer = &aw.writer,
    }) catch return ClientError.RequestFailed;

    return .{ .status = @intFromEnum(result.status), .body = aw.toOwnedSlice() catch return ClientError.RequestFailed };
}

const std = @import("std");
const Allocator = std.mem.Allocator;
const call_deadline = @import("call_deadline.zig");
const protocol = @import("contract").protocol;
