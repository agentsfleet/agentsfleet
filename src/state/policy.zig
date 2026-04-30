//! Policy decision recorder. Logs the decision and emits a policy_event
//! on the local event bus so any in-process subscriber can observe.

const std = @import("std");
const pg = @import("pg");
const types = @import("../types.zig");
const events = @import("../events/bus.zig");
const log = std.log.scoped(.policy);

pub fn recordPolicyEvent(
    conn: *pg.Conn,
    workspace_id: []const u8,
    run_id: ?[]const u8,
    action_class: types.ActionClass,
    decision: types.PolicyDecision,
    rule_id: []const u8,
    actor: []const u8,
) !void {
    _ = conn;
    log.info("policy.event workspace={s} class={s} decision={s} rule={s} actor={s}", .{
        workspace_id,
        @tagName(action_class),
        @tagName(decision),
        rule_id,
        actor,
    });
    events.emit("policy_event", run_id, "policy_event");
}
