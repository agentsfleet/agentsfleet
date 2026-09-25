//! lease_run_report.zig — submitting one run's terminal report, durably.
//!
//! Extracted from `lease_run.zig` as the terminal-report concern: rendering the
//! body, holding it where a later boot can find it, POSTing it, and spending the
//! hold once it lands. `executeAndReport` keeps the run; this keeps the answer.
//!
//! The ordering here is the whole point and reads top to bottom:
//!
//!   render → HOLD → post → release
//!
//! Holding BEFORE the POST is what makes a finished run outlive the process that
//! finished it. A failure handler only runs for a failure the process survives,
//! and a `SIGKILL`, an out-of-memory kill or a host reboot between the child
//! exiting and the acknowledgement arriving reaches none of them. Releasing only
//! AFTER the acknowledgement is the other half: releasing first would reopen the
//! window the hold exists to close.
//!
//! The body is rendered ONCE and those exact bytes are both held and sent, so a
//! replay carries the first attempt's `lease_id`, `event_id` and `fencing_token`
//! unchanged — the identity the report endpoint settles a repeat against without
//! charging twice.

/// Render, hold, POST and release one terminal report. Never fails the run: a
/// report that cannot be delivered is left held for the next boot's drain, and
/// every refusal is logged where it happened.
///
/// Borrows everything and owns nothing beyond the body it frees itself.
pub fn submit(
    io: std.Io,
    alloc: std.mem.Allocator,
    cp: *client_mod,
    runner_token: []const u8,
    cfg: Config,
    lease_id: []const u8,
    report: protocol.ReportRequest,
    spool: ?*ReportSpool,
) void {
    const body = std.json.Stringify.valueAlloc(alloc, report, .{}) catch |err| {
        log.err("report_encode_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .lease_id = lease_id, .err = @errorName(err) });
        return;
    };
    defer alloc.free(body);

    if (spool) |s| {
        if (s.hold(io, lease_id, body) == .unavailable) {
            // Already logged at error inside `hold`. The run still attempts its
            // POST: one in-memory attempt beats none, and the capacity gate in
            // `loop_spool.refuseWhenFull` stops the NEXT lease.
            log.warn("report_unspooled", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .lease_id = lease_id });
        }
    }

    cp.reportBody(alloc, runner_token, body, cfg.cp_deadlines.report_ms) catch |err| {
        // Not a lost result any more: it is on disk, and a later drain replays
        // these same bytes under the same identity.
        log.err("report_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .lease_id = lease_id, .err = @errorName(err) });
        // Back off so a down report endpoint cannot hot-spin the pool.
        sleepMs(io, constants.backoff.ms(0));
        return;
    };

    if (spool) |s| s.release(io, lease_id);
    log.debug("report_submitted", .{ .lease_id = lease_id, .outcome = @tagName(report.outcome) });
}

const std = @import("std");
const logging = @import("log");
const contract = @import("contract");
const constants = @import("common");
const client_errors = @import("../engine/client_errors.zig");
const client_mod = @import("control_plane_client.zig");
const Config = @import("config.zig");
const ReportSpool = @import("ReportSpool.zig");

const protocol = contract.protocol;
const log = logging.scoped(.fleet_runner);
const ERR_EXEC_TRANSPORT_LOSS = client_errors.ERR_EXEC_TRANSPORT_LOSS;
const sleepMs = constants.clock.sleepMs;
