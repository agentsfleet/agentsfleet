//! Repair-branch traffic is linkage traffic, never incident traffic.
//!
//! The repairer's own output comes back through this same webhook: the draft
//! Pull Request it opened, and the workflow runs on its branch. Left to the
//! normal filter those would wake the fleet as fresh incidents — the crew
//! investigating its own repair, one card per echo. So anything on an
//! `agentsfleet-repair/<event id>` branch is intercepted here: a PR opening
//! records the incident → PR linkage, a completed workflow run stamps the
//! deploy result, and everything else on such a branch is acknowledged and
//! dropped. Idempotent by construction (insert is DO NOTHING on the incident
//! key, the stamp is an absolute UPDATE), so it runs BEFORE the delivery
//! dedup and a GitHub redelivery changes nothing.

const std = @import("std");
const common_c = @import("common");
const hx_mod = @import("../hx.zig");
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");
const filter = @import("github_filter.zig");
const repair_links = @import("../../../state/repair_pr_links.zig");
const logging = @import("log");

const Hx = hx_mod.Hx;
const log = logging.scoped(.http_webhook_github);

const F_ACTION = "action";
const F_REPOSITORY = "repository";
const F_FULL_NAME = "full_name";
const S_ACTION_OPENED = "opened";
const S_ACTION_COMPLETED = "completed";
const S_CONCLUSION_SUCCESS = "success";
const IGNORED_REPAIR_BRANCH = "repair_branch";
const IGNORED_DUPLICATE_LINK = "duplicate_repair_link";
const IGNORED_UNLINKED_BRANCH = "unlinked_repair_branch";

pub const Interception = enum { not_repair, handled };

/// Intercept repair-branch deliveries. `.handled` means a response was
/// written and the delivery must not continue toward the event stream.
pub fn intercept(
    hx: Hx,
    event: []const u8,
    root: std.json.ObjectMap,
    fleet_id: []const u8,
    workspace_id: []const u8,
) Interception {
    if (std.mem.eql(u8, event, filter.EVENT_PULL_REQUEST)) return interceptPullRequest(hx, root, fleet_id, workspace_id);
    if (std.mem.eql(u8, event, filter.EVENT_WORKFLOW_RUN)) return interceptWorkflowRun(hx, root, fleet_id);
    return .not_repair;
}

fn interceptPullRequest(hx: Hx, root: std.json.ObjectMap, fleet_id: []const u8, workspace_id: []const u8) Interception {
    const pr = objectField(root, "pull_request") orelse return .not_repair;
    const head = objectField(pr, "head") orelse return .not_repair;
    const ref = stringField(head, "ref") orelse return .not_repair;
    if (!std.mem.startsWith(u8, ref, common_c.REPAIR_BRANCH_PREFIX)) return .not_repair;

    const action = stringField(root, F_ACTION) orelse "";
    if (!std.mem.eql(u8, action, S_ACTION_OPENED)) {
        // Sync/reopen/etc on the crew's own PR: acknowledged, never an incident.
        hx.ok(.ok, .{ .ignored = IGNORED_REPAIR_BRANCH });
        return .handled;
    }

    const repo = objectField(root, F_REPOSITORY) orelse return malformed(hx);
    const full_name = stringField(repo, F_FULL_NAME) orelse return malformed(hx);
    const number = intField(pr, "number") orelse return malformed(hx);
    const html_url = stringField(pr, "html_url") orelse return malformed(hx);
    const incident_event_id = ref[common_c.REPAIR_BRANCH_PREFIX.len..];

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return .handled;
    };
    defer hx.ctx.pool.release(conn);
    const outcome = repair_links.insert(hx.alloc, conn, .{
        .workspace_id = workspace_id,
        .fleet_id = fleet_id,
        .event_id = incident_event_id,
        .repository = full_name,
        .branch = ref,
        .pr_number = number,
        .pr_url = html_url,
    }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return .handled;
    };
    switch (outcome) {
        .inserted => {
            log.info("repair_pr_linked", .{ .fleet_id = fleet_id, .event_id = incident_event_id, .pr_number = number });
            hx.ok(.ok, .{ .linked = incident_event_id });
        },
        .duplicate => {
            // A second PR for an incident that already shipped one: the first
            // row is the record, and the surplus is named, never absorbed.
            log.warn("repair_pr_duplicate", .{ .error_code = ec.ERR_REPAIR_DUPLICATE_LINK, .fleet_id = fleet_id, .event_id = incident_event_id, .pr_number = number });
            hx.ok(.ok, .{ .ignored = IGNORED_DUPLICATE_LINK });
        },
    }
    return .handled;
}

fn interceptWorkflowRun(hx: Hx, root: std.json.ObjectMap, fleet_id: []const u8) Interception {
    const wr = objectField(root, "workflow_run") orelse return .not_repair;
    const head_branch = stringField(wr, "head_branch") orelse return .not_repair;
    if (!std.mem.startsWith(u8, head_branch, common_c.REPAIR_BRANCH_PREFIX)) return .not_repair;

    const action = stringField(root, F_ACTION) orelse "";
    if (!std.mem.eql(u8, action, S_ACTION_COMPLETED)) {
        hx.ok(.ok, .{ .ignored = IGNORED_REPAIR_BRANCH });
        return .handled;
    }
    // The stamp names the repository it came from: the linkage row records one,
    // and a branch name alone does not identify a repository.
    const repo = objectField(root, F_REPOSITORY) orelse return malformed(hx);
    const full_name = stringField(repo, F_FULL_NAME) orelse return malformed(hx);
    const conclusion = stringField(wr, "conclusion") orelse "";
    const status = if (std.mem.eql(u8, conclusion, S_CONCLUSION_SUCCESS))
        repair_links.DEPLOY_STATUS_OK
    else
        repair_links.DEPLOY_STATUS_FAILED;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return .handled;
    };
    defer hx.ctx.pool.release(conn);
    const stamped = repair_links.stampDeploy(conn, fleet_id, full_name, head_branch, status) catch {
        common.internalDbError(hx.res, hx.req_id);
        return .handled;
    };
    if (stamped) {
        log.info("repair_deploy_stamped", .{ .fleet_id = fleet_id, .repository = full_name, .branch = head_branch, .status = status });
        hx.ok(.ok, .{ .stamped = status });
    } else {
        // A repair-prefixed (repository, branch) with no linkage row: the PR
        // never opened, the run belongs to another repository, or it is another
        // fleet's repair. Acknowledged, nothing recorded.
        hx.ok(.ok, .{ .ignored = IGNORED_UNLINKED_BRANCH });
    }
    return .handled;
}

fn malformed(hx: Hx) Interception {
    hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MALFORMED_JSON);
    return .handled;
}

fn objectField(obj: std.json.ObjectMap, key: []const u8) ?std.json.ObjectMap {
    return switch (obj.get(key) orelse return null) {
        .object => |o| o,
        else => null,
    };
}

fn stringField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    return switch (obj.get(key) orelse return null) {
        .string => |s| s,
        else => null,
    };
}

fn intField(obj: std.json.ObjectMap, key: []const u8) ?i64 {
    return switch (obj.get(key) orelse return null) {
        .integer => |i| i,
        else => null,
    };
}
