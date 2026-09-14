//! loop_spool.zig — where the poll loop meets the report spool.
//!
//! Extracted from `loop.zig` as the spool concern (Module Split Pattern: a thin
//! adapter). Two rules live here, and they pull in opposite directions on
//! purpose:
//!
//!   - `refuseWhenFull` stops INTAKE. A daemon whose spool is full must not take
//!     another lease, because that would produce a result with nowhere durable
//!     to put it. The back-pressure lands on work not yet started, where waiting
//!     costs nothing, rather than on a finished run, where it costs everything.
//!   - `drainIfDue` keeps OUTPUT moving while the process is up, so a report
//!     held during a brief outage is delivered minutes later rather than waiting
//!     for a restart that may be days away.
//!
//! Both are no-ops without a spool: a daemon that could not claim a storage home
//! reports the way it always did, and said so once at boot.

/// True when the caller must NOT take a lease this pass, having already slept
/// out its backoff. Called at the top of a worker's poll.
pub fn refuseWhenFull(io: std.Io, spool: ?*ReportSpool, worker_index: u32, backoff_ms: u64) bool {
    const s = spool orelse return false;
    if (!s.atCapacity(io)) return false;
    log.warn("lease_refused_spool_full", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .index = worker_index });
    io.sleep(std.Io.Duration.fromMilliseconds(@intCast(backoff_ms)), .awake) catch return true;
    return true;
}

/// Replay held reports if this tick is due, folding the result back into the
/// cadence so a control plane that keeps refusing is knocked on less often.
/// Called once per control-loop tick, from the single-threaded control loop —
/// never from a worker, so two drains never race one entry.
pub fn drainIfDue(
    io: std.Io,
    alloc: std.mem.Allocator,
    cp: *client_mod,
    runner_token: []const u8,
    cfg: Config,
    spool: ?*ReportSpool,
    cadence: *Cadence,
) void {
    const s = spool orelse return;
    if (!cadence.due()) return;
    var plane = spool_delivery.Plane{
        .client = cp,
        .alloc = alloc,
        .runner_token = runner_token,
        .deadline_ms = cfg.cp_deadlines.report_ms,
    };
    const drained = s.drain(io, alloc, plane.delivery());
    cadence.observe(drained);
    if (drained.acknowledged + drained.kept + drained.superseded + drained.quarantined == 0) return;
    log.info("report_spool_drained", .{
        .acknowledged = drained.acknowledged,
        .kept = drained.kept,
        .superseded = drained.superseded,
        .quarantined = drained.quarantined,
    });
}

const std = @import("std");
const logging = @import("log");
const client_errors = @import("../engine/client_errors.zig");
const client_mod = @import("control_plane_client.zig");
const spool_delivery = @import("report_spool_delivery.zig");
const Config = @import("config.zig");
const ReportSpool = @import("ReportSpool.zig");

pub const Cadence = @import("report_spool_replay.zig").Cadence;

const log = logging.scoped(.fleet_runner);
const ERR_EXEC_TRANSPORT_LOSS = client_errors.ERR_EXEC_TRANSPORT_LOSS;
