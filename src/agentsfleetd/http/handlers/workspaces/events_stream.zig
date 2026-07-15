//! GET /v1/workspaces/{ws}/events/stream — ONE Server-Sent Events connection
//! carrying the live activity of every fleet the caller can read in a
//! workspace.
//!
//! Why it exists: the Fleets Wall used to open one SSE connection per live
//! tile, so a wall of L live fleets viewed by V operators cost L×V connections
//! and L×V stream-registry slots. This collapses that to one connection and one
//! slot per viewer.
//!
//! Shape (the per-fleet tail in `fleets/events_stream.zig` is the reference —
//! this is its generalization from one channel to a workspace-scoped set):
//!   1. Claim ONE registry slot for the whole connection, whatever the fleet
//!      count (cap or shutdown drain → refuse before any backend work).
//!   2. Authorize the workspace, then hand the socket to a dedicated detached
//!      thread — a stream must never park a handler-pool thread.
//!   3. The thread fans in: it subscribes to `fleet:{id}:activity` for each
//!      readable fleet through the hub's ONE shared Redis connection, so N
//!      fleets cost N map entries, never N dials.
//!   4. Every frame is tagged with the `fleet_id` of the channel it arrived on,
//!      so the client demultiplexes it to the right tile.
//!
//! Isolation is by CONSTRUCTION, not by filtering: only the authorized fleet
//! set is subscribed, so a frame from another workspace is never delivered to
//! this connection at all. There is deliberately no `PSUBSCRIBE fleet:*` — a
//! pattern subscribe would put every tenant's frames on one firehose and make
//! tenant isolation a matter of discipline instead of construction.
//!
//! Sequence ids are per-connection and reset to 0 on every connect; the server
//! ignores `Last-Event-ID`. A client recovers a reconnect gap through the
//! workspace events list (`GET /v1/workspaces/{ws}/events`), exactly as the
//! per-fleet stream's client does.

const std = @import("std");
const clock = @import("common").clock;
const httpz = @import("httpz");
const logging = @import("log");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const metrics = @import("../../../observability/metrics.zig");
const subscription_hub = @import("../../../events/subscription_hub.zig");
const activity_channel = @import("../../../events/activity_channel.zig");
const sse_frame = @import("../sse_frame.zig");
const FanIn = @import("events_stream_fanin.zig");

const log = logging.scoped(.http_workspace_events_stream);

const Hx = hx_mod.Hx;

/// SSE write buffer: one frame is six pieces (`id:`, seq, `event:`, kind,
/// `data:`, payload). An unbuffered writer syscalls on each; buffering them and
/// flushing once per frame is one syscall per frame with identical latency (the
/// flush fires the instant the frame is complete). Sized to hold a typical
/// activity frame whole; an over-large frame flushes mid-write and still lands.
const SSE_WRITE_BUF_LEN: usize = 8 * 1024;

/// The registry entry's fleet id for a workspace stream. The registry is keyed
/// `{workspace_id, fleet_id}` because every stream used to be per-fleet; a
/// workspace stream has no single fleet, so it registers under this sentinel —
/// distinct from any UUID, so an operator listing streams can tell the two
/// kinds apart at a glance.
const WORKSPACE_STREAM_FLEET_SENTINEL = "*";

pub fn innerWorkspaceEventsStream(
    hx: Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
) void {
    _ = req;
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }

    // Claim the slot before any backend work — shedding must stay cheap under a
    // tab-storm. ONE slot for the whole connection, regardless of how many
    // fleets it ends up fanning in (that is the entire point of this route).
    const reg_id = (hx.ctx.stream_registry.tryRegister(workspace_id, WORKSPACE_STREAM_FLEET_SENTINEL, clock.nowMillis(), hx.ctx.sse_max_streams) catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    }) orelse {
        metrics.incSseBackpressureRejections();
        log.warn("stream_cap_rejected", .{
            .error_code = ec.ERR_SSE_STREAM_CAP,
            .live = hx.ctx.stream_registry.count(),
            .max = hx.ctx.sse_max_streams,
        });
        hx.res.header(common.HEADER_RETRY_AFTER, common.RETRY_AFTER_BRIEF_VALUE);
        hx.fail(ec.ERR_SSE_STREAM_CAP, ec.MSG_SSE_STREAM_CAP);
        return;
    };
    var handed_off = false;
    defer if (!handed_off) hx.ctx.stream_registry.deregister(reg_id);

    if (!authorize(hx, workspace_id)) return;
    handed_off = startStreamThread(hx, workspace_id, reg_id);
}

fn authorize(hx: Hx, workspace_id: []const u8) bool {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return false;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return false;
    }
    return true;
}

/// Returns true when stream ownership (job + slot) transferred to the detached
/// thread; false when a response was written on the request path.
fn startStreamThread(hx: Hx, workspace_id: []const u8, reg_id: u64) bool {
    const job = StreamJob.create(hx.ctx, workspace_id, hx.principal, reg_id) catch |err| {
        switch (err) {
            // OOM or a hub already draining — the stream surface is momentarily
            // unavailable, not the client's fault (ECL: transient, not fatal).
            error.OutOfMemory, error.HubStopped => common.internalDbUnavailable(hx.res, hx.req_id),
        }
        return false;
    };
    hx.res.startEventStream(job, streamThreadMain) catch |err| {
        log.warn("sse_start_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
        job.destroy();
        return false;
    };
    return true;
}

fn streamThreadMain(job: *StreamJob, stream: std.Io.net.Stream) void {
    const ctx = job.ctx; // borrowed: boot-owned, outlives every stream thread
    const reg_id = job.reg_id;
    // LIFO teardown, mirroring the per-fleet stream: destroy the job (detach
    // every channel, release the shared fleet set), then the registry slot, and
    // close the socket LAST — an entry still in the registry guarantees its fd
    // is open, so a concurrent drain can never shutdown() a reused descriptor.
    defer stream.close(ctx.io);
    defer ctx.stream_registry.deregister(reg_id);
    defer job.destroy();
    ctx.stream_registry.attachFd(reg_id, stream.socket.handle);

    streamLoop(job, stream) catch |err| {
        // Most "errors" here are client disconnects mid-write (broken pipe).
        // The operator-visible event is the close, not the inner write error.
        log.debug("sse_stream_loop_exit", .{ .err = @errorName(err) });
    };
    log.debug("workspace_stream_closed", .{
        .workspace_id = job.fanin.workspace_id,
        .drops = job.fanin.sub.dropCount(),
    });
}

/// Everything the detached thread owns once the request returns. Allocated on
/// ctx.alloc, NOT the request arena — the arena dies when the handler returns,
/// the thread does not. Single owner: created on the request thread, destroyed
/// by the stream thread (or by `startStreamThread` when the spawn fails).
const StreamJob = struct {
    const Self = @This();

    ctx: *common.Context,
    fanin: *FanIn,
    reg_id: u64,

    const CreateError = error{ OutOfMemory, HubStopped };

    fn create(
        ctx: *common.Context,
        workspace_id: []const u8,
        caller: common.AuthPrincipal,
        reg_id: u64,
    ) CreateError!*StreamJob {
        const alloc = ctx.alloc;
        const job = alloc.create(StreamJob) catch return error.OutOfMemory;
        errdefer alloc.destroy(job);
        const fanin = try FanIn.create(ctx, workspace_id, caller);
        job.* = .{ .ctx = ctx, .fanin = fanin, .reg_id = reg_id };
        return job;
    }

    fn destroy(self: *Self) void {
        const alloc = self.ctx.alloc;
        self.fanin.destroy();
        alloc.destroy(self);
    }
};

/// Pop → write → heartbeat, with a fleet-set tick folded into the same loop.
///
/// One futex wait covers the WHOLE fan-in: every attached channel feeds the one
/// shared consumer queue, so the heartbeat cadence falls out of `pop`'s timeout
/// exactly as it does on the per-fleet stream — no N-queue poll, no idle spin.
fn streamLoop(job: *StreamJob, stream: std.Io.net.Stream) !void {
    const ctx = job.ctx;
    var seq: u64 = 0;
    // Buffered: one frame's pieces accumulate, then a single flush sends the
    // whole frame — one syscall per frame instead of one per piece, at
    // identical latency (the flush fires the moment the frame is complete).
    var wbuf: [SSE_WRITE_BUF_LEN]u8 = undefined;
    var w = stream.writer(ctx.io, &wbuf);
    // One knob drives both the re-authorization cadence and the fleet-set
    // staleness window, so a new fleet or a revoked member surfaces on the same
    // beat the cache refreshes on. The harness lowers it for the suites.
    const refresh_interval_ms = ctx.fleet_sets.refresh_interval_ms;
    var next_refresh_ms: i64 = 0; // 0 ⇒ the first pass syncs immediately

    while (true) {
        const now_ms = clock.nowMillis();
        if (now_ms >= next_refresh_ms) {
            if (!refreshFanIn(job, now_ms)) return;
            next_refresh_ms = now_ms + refresh_interval_ms;
        }

        // Wake for whichever comes first: the next heartbeat or the next fleet-set
        // tick. Sleeping the full heartbeat would stretch the refresh cadence to
        // the heartbeat's, so a new fleet would take longer to appear than the
        // cadence promises.
        switch (job.fanin.sub.pop(popWaitMs(next_refresh_ms))) {
            .message => |frame| {
                defer ctx.alloc.free(frame);
                // A dropped frame must not burn a sequence number — ids stay
                // gapless for the frames the client actually receives.
                if (try writeTagged(&w, seq, frame, job)) {
                    try w.interface.flush();
                    seq +%= 1;
                }
            },
            // A heartbeat write to a vanished client fails and unwinds the loop,
            // releasing the thread, the slot, and every subscription.
            .timeout => {
                try w.interface.writeAll(sse_frame.HEARTBEAT_FRAME);
                try w.interface.flush();
            },
            // Hub shutdown drain: exit promptly so stop() never waits on us.
            .closed => return,
        }
    }
}

/// The pop deadline: never past the next refresh tick, never longer than a
/// heartbeat. Saturates at zero so a passed deadline pops immediately rather
/// than underflowing into a very long wait.
fn popWaitMs(next_refresh_ms: i64) u64 {
    const remaining = next_refresh_ms - clock.nowMillis();
    if (remaining <= 0) return 0;
    return @min(@as(u64, @intCast(remaining)), @as(u64, sse_frame.HEARTBEAT_INTERVAL_MS));
}

/// One refresh tick. False ⇒ the stream must close (the caller lost access).
fn refreshFanIn(job: *StreamJob, now_ms: i64) bool {
    switch (job.fanin.sync(now_ms)) {
        .unchanged, .deferred => return true,
        .changed => |delta| {
            log.debug("workspace_stream_fleet_set_changed", .{
                .workspace_id = job.fanin.workspace_id,
                .added = delta.added,
                .removed = delta.removed,
                .fanned_in = job.fanin.channelCount(),
            });
            return true;
        },
        .revoked => {
            log.info("workspace_stream_revoked", .{ .workspace_id = job.fanin.workspace_id });
            return false;
        },
    }
}

/// Write one multiplexed frame: recover the originating fleet from the channel
/// the frame arrived on, read `kind` from the ORIGINAL payload, then splice the
/// `fleet_id` in.
///
/// Order matters: `extractKind` anchors on the payload's leading field, so the
/// kind must be read BEFORE the tag is spliced in front of it.
///
/// A frame whose channel or payload does not parse is DROPPED, never guessed —
/// mis-routing a frame to the wrong tile is worse than losing it, and the
/// client backfills the durable row anyway. False ⇒ dropped, nothing written.
fn writeTagged(w: anytype, seq: u64, frame: []const u8, job: *StreamJob) !bool {
    const tagged = subscription_hub.Subscription.splitTagged(frame) orelse {
        log.debug("workspace_stream_untagged_frame_dropped", .{ .workspace_id = job.fanin.workspace_id });
        return false;
    };
    const fleet_id = activity_channel.fleetId(tagged.channel_name) orelse {
        log.debug("workspace_stream_unroutable_frame_dropped", .{ .channel = tagged.channel_name });
        return false;
    };
    const kind = sse_frame.extractKind(tagged.payload) orelse sse_frame.DEFAULT_KIND;
    sse_frame.writeTaggedFrame(w, seq, kind, fleet_id, tagged.payload) catch |err| switch (err) {
        // Publisher shape drift: not a JSON object, so there is nothing to
        // splice into. Drop it rather than emit a malformed frame.
        error.NotAnObject => {
            log.debug("workspace_stream_malformed_payload_dropped", .{ .fleet_id = fleet_id });
            return false;
        },
        else => return err,
    };
    return true;
}

test {
    // The integration + soak suites are not production-imported, so discover
    // them here. sse_frame and events_stream_fanin are already imported at the
    // top of this file, so their test blocks are reachable through that.
    _ = @import("workspace_events_stream_integration_test.zig");
    _ = @import("workspace_stream_soak_test.zig");
}
