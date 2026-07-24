//! HTTP client the host daemon uses to drive the `/v1/runners/me/*` control
//! plane. It POSTs lease/heartbeat/report/activity to a `agentsfleetd` instance
//! using the same frozen `protocol` shapes the server speaks, so server and
//! client cannot drift. Enrollment is not a daemon concern (Option B): the
//! operator pre-mints the `agt_r` and the daemon authenticates with it directly.
//!
//! Uses the high-level `std.http.Client.fetch` (cross-platform; the manual
//! `open()`/`readVec()` path is Linux-broken under Zig 0.15) over ONE
//! persistent `std.http.Client` per owner (keep-alive connection reuse —
//! a chatty fleet run no longer pays a TCP/TLS handshake per frame).
//!
//! Every verb takes a required `deadline_ms`, armed against the ONE process
//! scheduler the runner root owns (`daemon/runner_deadline.zig`) — no
//! per-client watchdog thread. A fire shuts the in-flight pooled socket down,
//! so a hung control plane surfaces as a retryable transport error instead of
//! wedging the worker (and starving the child's deadline kill). Residual
//! window: name resolution + TCP connect inside fetch are not armed (the
//! production control plane is loopback/intra-region). The deadline targets a
//! connection GENERATION, never a descriptor number (`control_plane_deadline`),
//! and arming is fail-CLOSED — an unarmable verb is refused, never run
//! unbounded. `fetch` never follows a redirect (`send`): the new leg would dial
//! outside the armed generation and re-send the bearer cross-origin.

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
/// The process scheduler every call arms against. Borrowed from the runner
/// root; the client never owns, starts, or stops it.
sched: *deadline.Scheduler,

pub const ClientError = error{ RequestFailed, BadStatus, Unauthorized, MalformedResponse, SchedulerUnavailable };

/// Classify a control-plane HTTP status. 401/403 means the runner token was
/// rejected — a PERMANENT failure that retrying can never fix — so it maps to a
/// distinct `Unauthorized`, kept apart from a transient non-2xx (`BadStatus`).
/// The control loop fails loud on a rejected token instead of backing off
/// forever as generic transport loss (which hid a stale vault token as an
/// invisible `activating` crash-loop).
pub fn checkStatus(status: u16) ClientError!void {
    if (status == 401 or status == 403) return ClientError.Unauthorized;
    if (status < 200 or status >= 300) return ClientError.BadStatus;
}
// The shipped fire event, emitted from this owner (it knows the verb context).
const EV_DEADLINE_FIRED = "cp_call_deadline_fired";

/// Build a client with a persistent connection pool. `alloc` must outlive the
/// client (per-worker allocator); call `deinit()` to close pooled connections.
/// `sched` is the runner root's one process scheduler — borrowed, never owned.
pub fn init(alloc: Allocator, io: std.Io, sched: *deadline.Scheduler, base_url: []const u8) LoopbackClient {
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
        .sched = sched,
    };
}

pub fn deinit(self: *LoopbackClient) void {
    self.http.deinit();
    self.* = undefined;
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
    try checkStatus(res.status);
    return std.json.parseFromSlice(protocol.LeaseResponse, alloc, res.body, .{ .allocate = .alloc_always }) catch
        ClientError.MalformedResponse;
}

/// POST /v1/runners/me/heartbeats → signal liveness + receive fleet directives.
/// Request body is empty in S0 (capacity/version fields are a later workstream).
/// Returns the parsed HeartbeatResponse so the daemon can act on status==drain/stop.
pub fn heartbeat(self: *LoopbackClient, alloc: Allocator, runner_token: []const u8, deadline_ms: u31) !protocol.HeartbeatResponse {
    const res = try self.post(alloc, protocol.PATH_RUNNER_HEARTBEATS, runner_token, "", deadline_ms);
    defer alloc.free(res.body);
    try checkStatus(res.status);
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
    try checkStatus(res.status);
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
    try checkStatus(res.status);
}

/// GET /v1/runners/me/memory/{fleet_id} → the fleet's prior memory (a
/// compacted recency window). The parent seeds the child's in-run store from
/// this; the sandboxed child never makes the call. `.alloc_always` so the
/// returned deltas outlive `res.body` (freed here) — they ride the child input.
/// Caller deinits the parsed value after the run.
pub fn memoryHydrate(self: *LoopbackClient, alloc: Allocator, runner_token: []const u8, fleet_id: []const u8, deadline_ms: u31) !std.json.Parsed(protocol.MemoryHydrateResponse) {
    const path = try std.fmt.allocPrint(alloc, "{s}/{s}", .{ protocol.PATH_RUNNER_MEMORY, fleet_id });
    defer alloc.free(path);
    const res = try self.get(alloc, path, runner_token, deadline_ms);
    defer alloc.free(res.body);
    try checkStatus(res.status);
    return std.json.parseFromSlice(protocol.MemoryHydrateResponse, alloc, res.body, .{ .allocate = .alloc_always }) catch
        ClientError.MalformedResponse;
}

/// POST /v1/runners/me/memory/{fleet_id} → capture the run's memory for the
/// fleet. `lease_id` + `fencing_token` ride the body (like `report`) so the
/// control plane fences the write. Only the 2xx status matters to the caller;
/// the daemon swallows + logs a failure (a memory blip never fails the run).
pub fn memoryCapture(self: *LoopbackClient, alloc: Allocator, runner_token: []const u8, fleet_id: []const u8, req: protocol.MemoryPushRequest, deadline_ms: u31) !void {
    const path = try std.fmt.allocPrint(alloc, "{s}/{s}", .{ protocol.PATH_RUNNER_MEMORY, fleet_id });
    defer alloc.free(path);
    const payload = try std.json.Stringify.valueAlloc(alloc, req, .{});
    defer alloc.free(payload);
    const res = try self.post(alloc, path, runner_token, payload, deadline_ms);
    defer alloc.free(res.body);
    try checkStatus(res.status);
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

// The pure `/renew` classification half lives in `control_plane_client_renew.zig`
// (RULE FLL — this file sits at the line cap). Re-exported so callers keep using
// `client.RenewResult` / `client.classifyRenew` unchanged.
const renew_mod = @import("control_plane_client_renew.zig");
pub const TerminalRenew = renew_mod.TerminalRenew;
pub const RenewResult = renew_mod.RenewResult;
pub const classifyRenew = renew_mod.classifyRenew;
pub const isTerminalRenewStatus = renew_mod.isTerminalRenewStatus;

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

// `cp.mint` lives in `control_plane_client_mint.zig` (RULE FLL — at the cap).
const mint_mod = @import("control_plane_client_mint.zig");
pub const MintOutcome = mint_mod.MintOutcome;
pub const mint = mint_mod.mint;

pub const PostResult = struct { status: u16, body: []u8 };

/// Pin the pooled connection the next fetch will use (get-or-create, then
/// release back to the free list so the fetch pops the same one) and return
/// its socket handle for the attempt to arm. Null when the connect fails —
/// `send` then refuses the verb fail-closed.
fn pooledHandle(self: *LoopbackClient) ?std.Io.net.Socket.Handle {
    // Pins the socket the deadline is armed against BEFORE `send`'s fetch;
    // `http_pin` owns the prime-then-connect discipline shared with the
    // daemon's connector/broker sites (an unprimed handshake panics on a null
    // `client.now.?` — the runner never heartbeats → crash-loops).
    return http_pin.connectPinned(&self.http, self.host, self.port, self.tls);
}

/// Shared core of the bearer-authed verbs: arm one scheduler guard against this
/// attempt's connection generation, issue one `fetch` on the persistent client,
/// and return the status + owned response body.
/// `payload == null` sends no body (GET); a non-null payload rides a POST with a
/// content-type header. The single `errdefer` here releases the partial response
/// buffer on any mid-stream fetch failure — the success path hands it off via
/// `toOwnedSlice()`. `post`/`get`/`mint` are thin wrappers over this.
fn send(
    self: *LoopbackClient,
    alloc: Allocator,
    method: std.http.Method,
    path: []const u8,
    bearer: []const u8,
    payload: ?[]const u8,
    deadline_ms: u31,
) !PostResult {
    // Stack-local control block, armed by generation rather than descriptor.
    // Fail the verb closed if the deadline can't be enforced (M100): both
    // refusal branches return, so no path reaches an unarmed fetch.
    var attempt: deadline.Attempt = .{};
    attempt.begin();
    defer attempt.release();
    switch (attempt.armPinned(self.sched, self.pooledHandle(), deadline_ms)) {
        .armed => {},
        .pin_failed => return ClientError.RequestFailed,
        .scheduler_unavailable => return ClientError.SchedulerUnavailable,
    }

    const url = try std.fmt.allocPrint(alloc, "{s}{s}", .{ self.base_url, path });
    defer alloc.free(url);
    const auth = try std.fmt.allocPrint(alloc, "Bearer {s}", .{bearer});
    defer alloc.free(auth);

    // BUFFER GATE: ArrayList response body — fetch appends as it streams; read once for the parse.
    var body: std.ArrayList(u8) = .empty;
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &body);
    errdefer aw.deinit(); // release partial bytes if fetch fails mid-stream; success path uses toOwnedSlice

    // authorization is always sent; a body-bearing POST adds content-type. The
    // fixed buffer outlives the fetch; `extra_headers` takes the used prefix.
    var header_buf: [2]std.http.Header = .{
        .{ .name = "authorization", .value = auth },
        .{ .name = "content-type", .value = "application/json" },
    };
    const headers: []const std.http.Header = if (payload == null) header_buf[0..1] else header_buf[0..2];

    const result = self.http.fetch(.{
        .location = .{ .url = url },
        .method = method,
        .payload = payload,
        .extra_headers = headers,
        .redirect_behavior = .unhandled, // see module doc: never follow (armed-generation + bearer leak)
        .response_writer = &aw.writer,
    }) catch {
        // The owner's flag is what distinguishes "we cancelled it" from an
        // ordinary transport failure; both stay `RequestFailed` so the control
        // loop's retry classification is unchanged.
        if (attempt.wasInterrupted())
            log.warn(EV_DEADLINE_FIRED, .{ .error_code = client_errors.ERR_EXEC_TRANSPORT_LOSS, .deadline_ms = deadline_ms });
        return ClientError.RequestFailed;
    };

    return .{ .status = @intFromEnum(result.status), .body = aw.toOwnedSlice() catch return ClientError.RequestFailed };
}

/// One bearer-authed POST on the persistent client. Returns the status +
/// response body (owned by `alloc`). Pub so the split-out `mint` verb shares it.
pub fn post(self: *LoopbackClient, alloc: Allocator, path: []const u8, bearer: []const u8, payload: []const u8, deadline_ms: u31) !PostResult {
    return self.send(alloc, .POST, path, bearer, payload, deadline_ms);
}

/// One bearer-authed GET (no body) on the persistent client. Returns the status +
/// response body (owned by `alloc`). The shared GET primitive: wrapped by the
/// read-only `getSelf`/`memoryHydrate` verbs and consumed by `bundle_extract` for
/// the Fleet Bundle snapshot download.
pub fn get(self: *LoopbackClient, alloc: Allocator, path: []const u8, bearer: []const u8, deadline_ms: u31) !PostResult {
    return self.send(alloc, .GET, path, bearer, null, deadline_ms);
}

const std = @import("std");
const Allocator = std.mem.Allocator;
const logging = @import("log");
const deadline = @import("control_plane_deadline.zig");
const http_pin = @import("http_pin");
const client_errors = @import("../engine/client_errors.zig");
const protocol = @import("contract").protocol;

const log = logging.scoped(.fleet_runner);
