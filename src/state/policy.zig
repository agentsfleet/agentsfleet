//! Policy enforcement — records policy_decision events in policy_events table.
//! M1: permissive mode — classify all actions and record as 'allow'.
//! Provides the audit trail required by M1_003 AC#1 and AC#4.
//! POLICY_ENFORCE=strict (future M2) enables full gate enforcement.

const std = @import("std");
const pg = @import("pg");
const types = @import("../types.zig");
const events = @import("../events/bus.zig");
const log = std.log.scoped(.policy);

/// Record a policy_decision event in the policy_events table.
///
/// Called before every state-changing handler (sensitive/critical class).
/// M1 permissive mode: decision is always 'allow' for authenticated requests.
/// Denied paths must pass .deny explicitly.
pub fn recordPolicyEvent(
    conn: *pg.Conn,
    workspace_id: []const u8,
    run_id: ?[]const u8,
    action_class: types.ActionClass,
    decision: types.PolicyDecision,
    rule_id: []const u8,
    actor: []const u8,
) !void {
    const now_ms = std.time.milliTimestamp();
    var r = try conn.query(
        \\INSERT INTO policy_events
        \\  (run_id, workspace_id, action_class, decision, rule_id, actor, ts)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7)
    , .{
        run_id,
        workspace_id,
        @tagName(action_class),
        @tagName(decision),
        rule_id,
        actor,
        now_ms,
    });
    r.deinit();

    var request_id: []const u8 = "-";
    if (run_id) |rid| {
        var rq = conn.query("SELECT request_id FROM runs WHERE run_id = $1", .{rid}) catch null;
        if (rq) |*q| {
            defer q.deinit();
            if ((q.next() catch null)) |rrow| {
                if (rrow.get(?[]u8, 0) catch null) |req| {
                    if (req.len > 0) request_id = req;
                }
            }
        }
    }
    log.info("policy_event request_id={s} workspace={s} class={s} decision={s} rule={s} actor={s}", .{
        request_id,
        workspace_id,
        @tagName(action_class),
        @tagName(decision),
        rule_id,
        actor,
    });
    var detail_buf: [192]u8 = undefined;
    const detail = std.fmt.bufPrint(
        &detail_buf,
        "request_id={s} workspace={s} class={s} decision={s} rule={s} actor={s}",
        .{ request_id, workspace_id, @tagName(action_class), @tagName(decision), rule_id, actor },
    ) catch "policy_event";
    events.emit("policy_event", run_id, detail);
}
