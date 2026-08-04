//! child_supervisor_read.zig — the framed-stdout read loop (parent side).
//!
//! Split out of `child_supervisor.zig` to keep both files within the line
//! budget: this module owns the child→parent message plane — the activity /
//! memory / usage sinks, the renewal hook the daemon installs, and the loop
//! that reads framed stdout up to the terminal `result` frame while driving
//! renewal ticks. `child_supervisor.zig` re-exports the public names so callers
//! and tests keep using `child_supervisor.{ActivitySink,RenewHook,readResult,…}`.
//!
//! One thread runs this loop: frame parsing and the renewal `onTick` never race,
//! so the folded usage snapshot is a plain field (no atomics).

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const contract = @import("contract");
const pipe_proto = @import("pipe_proto.zig");
const cred = @import("engine/credential_request.zig");
const fetch_req = @import("engine/repo_fetch_request.zig");
const result_mod = @import("child_supervisor_result.zig");
const renew_mod = @import("child_supervisor_renew.zig");
const types = @import("engine/types.zig");
const client_errors = @import("engine/client_errors.zig");

const log = logging.scoped(.runner_supervisor);
const ERR_EXEC_TRANSPORT_LOSS = client_errors.ERR_EXEC_TRANSPORT_LOSS;

const ActivityFrame = contract.activity.ActivityFrame;
pub const ReadOutcome = result_mod.ReadOutcome;

/// Cap on the serialized result we read back from a child (defensive against a
/// runaway child flooding stdout).
const MAX_RESULT_BYTES: usize = 8 * 1024 * 1024;

/// Best-effort sink for the `activity` frames the child streams while running.
/// The parent forwards each to the control plane (`POST .../activity`); a
/// dropped frame is cosmetic (the durable record is `report`), so `forward`
/// returns void and never fails the lease.
pub const ActivitySink = struct {
    ctx: *anyopaque,
    forward: *const fn (ctx: *anyopaque, frame: ActivityFrame) void,
};

/// Best-effort sink for the child's `.memory` capture frames. `payload` is the
/// raw frame bytes (a JSON array of `MemoryDelta`); the daemon parses + POSTs
/// them. A dropped frame is recoverable (the next capture re-sends the full
/// set), so `forward` returns void and never fails the lease.
pub const MemorySink = struct {
    ctx: *anyopaque,
    forward: *const fn (ctx: *anyopaque, payload: []const u8) void,
};

// The renewal surface lives in its own module (RULE FLL split) and depends on
// nothing here, so it is imported rather than defined. Re-exported so every
// existing `child_supervisor.Renew*` reference keeps resolving.
pub const RenewDecision = renew_mod.RenewDecision;
pub const RenewHook = renew_mod.RenewHook;
pub const RenewTick = renew_mod.RenewTick;
const RenewPump = renew_mod.RenewPump;

/// Outcome of servicing one `credential_request` (M102 §3): a short-lived token
/// for the child, or a typed rejection it fails closed on. `token` is owned by the
/// `alloc` handed to `onMint`; the read loop frees it after framing the reply.
pub const CredentialOutcome = union(enum) {
    minted: struct { token: []const u8, expires_at_ms: i64 },
    rejected,
};

/// Outcome of servicing one `repo_fetch_request` (M157 §4): a workspace-relative
/// path to a ready working tree, or a named refusal the child reformulates
/// against.
///
/// Both slices are BORROWED, valid for the synchronous `onFetch` call only — the
/// read loop frames them and forgets them, and frees neither. Every real value
/// is a static string (a `Refusal.reason()`, a `Failure.reason()`, or the fetch
/// target's fixed name), so there is nothing here to own and no failure path on
/// which the reply itself could fail to allocate.
///
/// Neither arm ever carries a credential: the token authenticated the fetch
/// daemon-side and stops there (Invariant 9).
pub const FetchOutcome = union(enum) {
    ready: []const u8,
    refused: []const u8,
};

/// Hook the daemon installs so the supervisor can fetch a repository on the
/// child's behalf without the read loop knowing any git. `onFetch` validates the
/// ask against the lease's binding, mints, and fetches into the workspace the
/// daemon derives from `lease_id` — the child supplies none of those, exactly as
/// it supplies no workspace on the mint channel (Invariant 2). A null hook means
/// fetching is unconfigured, and every ask is refused.
pub const FetchHook = struct {
    ctx: *anyopaque,
    onFetch: *const fn (
        ctx: *anyopaque,
        alloc: std.mem.Allocator,
        repository: []const u8,
        commit: []const u8,
        head: []const u8,
        tick: ?RenewTick,
    ) FetchOutcome,
};

/// Hook the daemon installs so the supervisor can mint on the child's behalf
/// without the read loop knowing any HTTP. `onMint` forwards the ask to the
/// daemon broker over the agt_r plane (`control_plane_client.mint`), binding the
/// mint to the lease's workspace server-side (Invariant 2). It never logs the
/// token (VLT). A null hook means mint is unconfigured — every ask is rejected.
pub const MintHook = struct {
    ctx: *anyopaque,
    onMint: *const fn (ctx: *anyopaque, alloc: std.mem.Allocator, integration: []const u8, scope: ?[]const u8) CredentialOutcome,
};

/// Read the child's framed stdout up to the terminal `result` frame, bounded by
/// the lease deadline. Each `activity` frame is forwarded best-effort and freed;
/// the `result` frame's bytes are returned (caller-owned). EOF before a result
/// yields empty bytes (the caller classifies that as a transport loss); deadline
/// elapse sets `timed_out` and the caller kills the child.
pub fn readResult(
    alloc: std.mem.Allocator,
    fd: std.posix.fd_t,
    /// Child's stdin (parent→child): where a `credential_response` is framed back
    /// when the child raises a `credential_request`. The lease's response channel.
    response_fd: std.posix.fd_t,
    deadline_ms: i64,
    sink: ActivitySink,
    mem_sink: MemorySink,
    renew_hook: ?RenewHook,
    /// Services on-demand mint asks (M102 §3); null ⇒ every ask is rejected.
    mint_hook: ?MintHook,
    /// Services on-demand repository fetches (M157 §4); null ⇒ every ask is refused.
    fetch_hook: ?FetchHook,
) !ReadOutcome {
    var deadline = deadline_ms;
    // Frame parsing and renewal ticks share this one read-loop thread (every
    // onTick runs between reads) — plain fields, no atomics; @max-fold = no regress.
    var usage = pipe_proto.UsageSnapshot{};
    while (true) {
        const tick_deadline = if (renew_hook) |h|
            @min(deadline, clock.nowMillis() + h.tick_ms)
        else
            deadline;
        switch (try pipe_proto.waitReadable(fd, tick_deadline)) {
            .timed_out => {
                const now = clock.nowMillis();
                if (now >= deadline) return .{ .timed_out = true };
                if (applyTick(renew_hook, &deadline, now, usage)) |reason| return .{ .terminated = true, .terminate_reason = reason };
                continue;
            },
            .readable => {},
        }
        // Data is present: read one whole frame at the full lease deadline so a
        // tick never interrupts a frame mid-read (which would desync the stream).
        switch (try pipe_proto.readFrame(alloc, fd, deadline, MAX_RESULT_BYTES)) {
            .timed_out => return .{ .timed_out = true },
            .eof => return .{},
            .frame => |f| if (handleFrame(alloc, f, response_fd, sink, mem_sink, renew_hook, mint_hook, fetch_hook, &deadline, &usage)) |outcome|
                return outcome,
        }
    }
}

/// Dispatch one decoded non-control frame: forward it to its sink (or fold a
/// usage snapshot), then run the renewal tick. Returns a terminal `ReadOutcome`
/// to propagate — a `result` frame's bytes (ownership transfers to the caller),
/// or a hook `.terminate` — else null to keep reading. The `activity`/`memory`/
/// `usage` payloads are freed here; the `result` payload is not.
fn handleFrame(
    alloc: std.mem.Allocator,
    f: pipe_proto.Frame,
    response_fd: std.posix.fd_t,
    sink: ActivitySink,
    mem_sink: MemorySink,
    renew_hook: ?RenewHook,
    mint_hook: ?MintHook,
    fetch_hook: ?FetchHook,
    deadline: *i64,
    usage: *pipe_proto.UsageSnapshot,
) ?ReadOutcome {
    switch (f.ftype) {
        .activity => {
            defer alloc.free(f.payload);
            forwardActivity(alloc, sink, f.payload);
        },
        .memory => {
            defer alloc.free(f.payload);
            // Parent POSTs the capture bytes; the frame also attests liveness.
            mem_sink.forward(mem_sink.ctx, f.payload);
        },
        .usage => {
            defer alloc.free(f.payload);
            if (pipe_proto.UsageSnapshot.decode(f.payload)) |snap|
                usage.fold(snap)
            else
                // A malformed 24-byte frame means real wire corruption / version
                // skew (an old child sends NO usage frame, never a bad one), so
                // warn — symmetric with the child-side usage_frame_write_failed.
                log.warn("usage_frame_dropped", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .len = f.payload.len });
        },
        .credential_request => {
            defer alloc.free(f.payload);
            // Mint on the child's behalf and frame the reply back down its stdin.
            // The child is blocked reading that reply, so no stdout frame races.
            serviceCredentialRequest(alloc, f.payload, response_fd, mint_hook);
        },
        .repo_fetch_request => {
            defer alloc.free(f.payload);
            // Same shape as the mint ask: the child is blocked reading its reply,
            // so no stdout frame races this. Unlike the mint, the fetch is
            // minutes-scale — so it carries a renewal pump, because this loop is
            // the only thing that renews the lease and it is about to be busy.
            var pump = RenewPump{ .hook = renew_hook, .usage = usage, .deadline = deadline };
            serviceFetchRequest(alloc, f.payload, response_fd, fetch_hook, pump.tick());
        },
        .result => return .{ .bytes = f.payload },
        // These three are parent→child only — the parent never reads them off the
        // child's stdout. One here is wire skew; drop it.
        .lease, .credential_response, .repo_fetch_response => {
            defer alloc.free(f.payload);
            log.warn("unexpected_child_frame", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .ftype = @tagName(f.ftype) });
        },
    }
    // Every non-terminal frame attests liveness and is a renewal point.
    if (applyTick(renew_hook, deadline, clock.nowMillis(), usage.*)) |reason| return .{ .terminated = true, .terminate_reason = reason };
    return null;
}

/// Parse one `credential_request` payload, mint via the hook, and frame the
/// `credential_response` back to the child's stdin. Best-effort + fail-closed:
/// any parse miss, a null hook, or a broker rejection frames `ok=false`, and the
/// child aborts its tool call. The token (when minted) is owned by `alloc` — freed
/// here right after framing — and is never logged (VLT).
fn serviceCredentialRequest(
    alloc: std.mem.Allocator,
    payload: []const u8,
    response_fd: std.posix.fd_t,
    mint_hook: ?MintHook,
) void {
    const hook = mint_hook orelse return writePipeResponse(alloc, response_fd, .{ .ok = false });
    const parsed = std.json.parseFromSlice(cred.PipeRequest, alloc, payload, .{}) catch
        return writePipeResponse(alloc, response_fd, .{ .ok = false });
    defer parsed.deinit();
    switch (hook.onMint(hook.ctx, alloc, parsed.value.integration, parsed.value.scope)) {
        .minted => |m| {
            defer alloc.free(m.token);
            writePipeResponse(alloc, response_fd, .{ .ok = true, .token = m.token, .expires_at_ms = m.expires_at_ms });
        },
        .rejected => writePipeResponse(alloc, response_fd, .{ .ok = false }),
    }
}

/// Parse one `repo_fetch_request`, service it through the hook, and frame the
/// `repo_fetch_response` back to the child's stdin. Fail-closed: a parse miss, a
/// null hook, or a refusal all frame `ok=false` with a reason, and the child's
/// tool call reports it rather than proceeding against a tree that is not there.
/// The hook's slices are borrowed for the call and freed by nobody.
fn serviceFetchRequest(
    alloc: std.mem.Allocator,
    payload: []const u8,
    response_fd: std.posix.fd_t,
    fetch_hook: ?FetchHook,
    tick: ?RenewTick,
) void {
    const hook = fetch_hook orelse
        return writeFetchResponse(alloc, response_fd, .{ .ok = false, .reason = REASON_FETCH_UNCONFIGURED });
    const parsed = std.json.parseFromSlice(fetch_req.PipeRequest, alloc, payload, .{}) catch
        return writeFetchResponse(alloc, response_fd, .{ .ok = false, .reason = REASON_FETCH_MALFORMED_ASK });
    defer parsed.deinit();

    switch (hook.onFetch(hook.ctx, alloc, parsed.value.repository, parsed.value.commit, parsed.value.head, tick)) {
        .ready => |path| writeFetchResponse(alloc, response_fd, .{ .ok = true, .path = path }),
        .refused => |reason| writeFetchResponse(alloc, response_fd, .{ .ok = false, .reason = reason }),
    }
}

/// Serialize + frame a `repo_fetch_response` to the child's stdin. Best-effort,
/// for the same reason the credential reply is: a wedged pipe leaves the child to
/// time out on its own bounded read rather than hanging the parent.
fn writeFetchResponse(alloc: std.mem.Allocator, response_fd: std.posix.fd_t, resp: fetch_req.PipeResponse) void {
    const json = std.json.Stringify.valueAlloc(alloc, resp, .{}) catch return;
    defer alloc.free(json);
    pipe_proto.writeFrame(response_fd, .repo_fetch_response, json) catch |err|
        log.warn("repo_fetch_response_write_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
}

/// Refusals this layer raises itself, before any hook runs (RULE UFS).
const REASON_FETCH_UNCONFIGURED = "repository fetch is not configured for this lease";
const REASON_FETCH_MALFORMED_ASK = "repository fetch ask could not be parsed";

/// Serialize + frame a `credential_response` to the child's stdin. Best-effort:
/// a write failure leaves the child to time out on its read and fail closed (its
/// round-trip is bounded by the lease deadline), so a wedged pipe never hangs the
/// parent. The token, when present, is framed straight through — never logged.
fn writePipeResponse(alloc: std.mem.Allocator, response_fd: std.posix.fd_t, resp: cred.PipeResponse) void {
    const json = std.json.Stringify.valueAlloc(alloc, resp, .{}) catch return;
    defer alloc.free(json);
    pipe_proto.writeFrame(response_fd, .credential_response, json) catch |err|
        log.warn("credential_response_write_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
}

/// Ask the renewal hook for a decision and apply it to `deadline`. Returns true
/// iff the child must be terminated (lease lost / capped / no credits). A null
/// hook (no renewal configured) is a no-op.
/// Run one renewal tick. Returns the class to terminate under, or `null` to keep
/// reading — an optional rather than a bool so the hook's reason survives to
/// `classify` instead of being flattened to "something stopped us".
fn applyTick(renew_hook: ?RenewHook, deadline: *i64, now_ms: i64, usage: pipe_proto.UsageSnapshot) ?types.FailureClass {
    const h = renew_hook orelse return null;
    switch (h.onTick(h.ctx, now_ms, usage)) {
        .keep => {},
        .extend => |new_deadline| deadline.* = new_deadline,
        .terminate => |reason| return reason,
    }
    return null;
}

/// Parse one `activity` frame payload and hand it to the sink. Best-effort: a
/// malformed frame is dropped (activity is cosmetic). The parsed frame's slices
/// borrow `arena`, valid for the synchronous `forward` call.
fn forwardActivity(alloc: std.mem.Allocator, sink: ActivitySink, payload: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const frame = std.json.parseFromSliceLeaky(ActivityFrame, arena.allocator(), payload, .{}) catch return;
    sink.forward(sink.ctx, frame);
}
