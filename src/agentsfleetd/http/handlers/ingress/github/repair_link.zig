//! Repair-branch traffic is durable evidence and never a new incident.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");

const hx_mod = @import("../../hx.zig");
const common = @import("../../common.zig");
const repair_branch = @import("../../../../git/repair_branch.zig");
const ec = @import("../../../../errors/error_registry.zig");
const filter = @import("../../webhooks/github_filter.zig");
const repair_evidence = @import("../../../../state/repair_evidence.zig");
const event_time = @import("../../../../state/fleet_events_filter.zig");
const gate_resolve = @import("repair_gate_resolve.zig");
const provenance = @import("repair_link_provenance.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.http_webhook_github);

const FIELD_ACTION = "action";
const FIELD_REPOSITORY = "repository";
const FIELD_FULL_NAME = "full_name";
const FIELD_PULL_REQUEST = "pull_request";
const FIELD_WORKFLOW_RUN = "workflow_run";
const FIELD_HEAD = "head";
const FIELD_REF = "ref";
const FIELD_HEAD_BRANCH = "head_branch";
const FIELD_NUMBER = "number";
const FIELD_HTML_URL = "html_url";
const FIELD_MERGED = "merged";
const FIELD_MERGE_COMMIT_SHA = "merge_commit_sha";
const FIELD_MERGED_AT = "merged_at";
const FIELD_ID = "id";
const FIELD_NAME = "name";
const FIELD_HEAD_SHA = "head_sha";
const FIELD_CONCLUSION = "conclusion";
const FIELD_UPDATED_AT = "updated_at";

const ACTION_OPENED = "opened";
const ACTION_CLOSED = "closed";
const ACTION_COMPLETED = "completed";
const IGNORED_REPAIR_BRANCH = "repair_branch";
const IGNORED_DUPLICATE_LINK = "duplicate_repair_link";
const IGNORED_INVALID_REFERENCE = "invalid_repair_reference";
const IGNORED_PROVENANCE = "repair_provenance_refused";
const IGNORED_UNMERGED = "unmerged_repair_pr";
const RECORDED_REPAIR_RUN = "repair_run_recorded";
const REPLAYED_REPAIR_RUN = "repair_run_replayed";

pub const Interception = enum { not_repair, handled };

/// Per-Fleet signed route. The path Fleet is an additional owner fence.
pub fn intercept(
    hx: Hx,
    event: []const u8,
    root: std.json.ObjectMap,
    fleet_id: []const u8,
    workspace_id: []const u8,
) Interception {
    const branch = branchForEvent(event, root) orelse return .not_repair;
    if (!std.mem.startsWith(u8, branch, repair_branch.PREFIX)) return .not_repair;
    const repository = repositoryName(root) orelse return malformed(hx);
    const conn = hx.ctx.pool.acquire() catch return dbUnavailable(hx);
    defer hx.ctx.pool.release(conn);
    return resolveAndHandle(hx, conn, event, root, workspace_id, .{ .fleet = fleet_id }, repository, branch);
}

/// Shared signed App ingress. The mapped workspace and installation are already
/// signature-qualified; the compact gate still resolves the exact Fleet/event.
pub fn interceptIngress(
    hx: Hx,
    conn: *pg.Conn,
    event: []const u8,
    root: std.json.ObjectMap,
    workspace_id: []const u8,
    installation_id: []const u8,
    repository: []const u8,
) Interception {
    const branch = branchForEvent(event, root) orelse return .not_repair;
    if (!std.mem.startsWith(u8, branch, repair_branch.PREFIX)) return .not_repair;
    return resolveAndHandle(hx, conn, event, root, workspace_id, .{ .installation = installation_id }, repository, branch);
}

fn resolveAndHandle(
    hx: Hx,
    conn: *pg.Conn,
    event: []const u8,
    root: std.json.ObjectMap,
    workspace_id: []const u8,
    authority: gate_resolve.Authority,
    repository: []const u8,
    branch: []const u8,
) Interception {
    var resolved = gate_resolve.resolve(hx.alloc, conn, branch, workspace_id, authority, repository) catch return dbFailure(hx);
    switch (resolved) {
        .invalid_reference => return ignored(hx, IGNORED_INVALID_REFERENCE),
        .refused => {
            log.warn("repair_provenance_refused", .{ .error_code = ec.ERR_REPAIR_PROVENANCE_REFUSED, .workspace_id = workspace_id, .repository = repository });
            return ignored(hx, IGNORED_PROVENANCE);
        },
        .owner => |*owner| {
            defer owner.deinit(hx.alloc);
            if (std.mem.eql(u8, event, filter.EVENT_PULL_REQUEST)) {
                return handlePullRequest(hx, conn, root, owner, repository, branch);
            }
            if (std.mem.eql(u8, event, filter.EVENT_WORKFLOW_RUN)) {
                return handleWorkflowRun(hx, conn, root, owner, repository, branch);
            }
            return .not_repair;
        },
    }
}

fn handlePullRequest(hx: Hx, conn: *pg.Conn, root: std.json.ObjectMap, owner: *const gate_resolve.Owner, repository: []const u8, branch: []const u8) Interception {
    if (!provenance.ownedPullRequest(root, repository, owner.base_branch, hx.ctx.github_app_slug orelse "")) {
        return ignored(hx, IGNORED_PROVENANCE);
    }
    const pull_request = objectField(root, FIELD_PULL_REQUEST) orelse return malformed(hx);
    const action = stringField(root, FIELD_ACTION) orelse return malformed(hx);
    if (std.mem.eql(u8, action, ACTION_OPENED)) {
        return insertLink(hx, conn, pull_request, owner, repository, branch);
    }
    if (std.mem.eql(u8, action, ACTION_CLOSED)) {
        return recordMerge(hx, conn, pull_request, owner, repository, branch);
    }
    return ignored(hx, IGNORED_REPAIR_BRANCH);
}

fn insertLink(hx: Hx, conn: *pg.Conn, pull_request: std.json.ObjectMap, owner: *const gate_resolve.Owner, repository: []const u8, branch: []const u8) Interception {
    const number = intField(pull_request, FIELD_NUMBER) orelse return malformed(hx);
    const html_url = stringField(pull_request, FIELD_HTML_URL) orelse return malformed(hx);
    const outcome = repair_evidence.recordOpened(hx.alloc, conn, .{
        .workspace_id = owner.workspace_id,
        .fleet_id = owner.fleet_id,
        .event_id = owner.event_id,
        .repository = repository,
        .branch = branch,
        .pr_number = number,
        .pr_url = html_url,
    }) catch return dbFailure(hx);
    if (outcome == .inserted) {
        log.info("repair_pr_linked", .{ .fleet_id = owner.fleet_id, .event_id = owner.event_id, .pr_number = number });
        hx.ok(.ok, .{ .linked = owner.event_id });
    } else {
        log.warn("repair_pr_duplicate", .{ .error_code = ec.ERR_REPAIR_DUPLICATE_LINK, .fleet_id = owner.fleet_id, .event_id = owner.event_id, .pr_number = number });
        hx.ok(.ok, .{ .ignored = IGNORED_DUPLICATE_LINK });
    }
    return .handled;
}

fn recordMerge(hx: Hx, conn: *pg.Conn, pull_request: std.json.ObjectMap, owner: *const gate_resolve.Owner, repository: []const u8, branch: []const u8) Interception {
    if (!(boolField(pull_request, FIELD_MERGED) orelse false)) return ignored(hx, IGNORED_UNMERGED);
    const number = intField(pull_request, FIELD_NUMBER) orelse return malformed(hx);
    const html_url = nonEmpty(stringField(pull_request, FIELD_HTML_URL)) orelse return malformed(hx);
    const sha = nonEmpty(stringField(pull_request, FIELD_MERGE_COMMIT_SHA)) orelse return ignored(hx, IGNORED_UNMERGED);
    const merged_at_text = nonEmpty(stringField(pull_request, FIELD_MERGED_AT)) orelse return ignored(hx, IGNORED_UNMERGED);
    const merged_at = providerTimestamp(merged_at_text) orelse return ignored(hx, IGNORED_UNMERGED);
    const arrival = repair_evidence.recordMerge(hx.alloc, conn, .{
        .link = .{
            .workspace_id = owner.workspace_id,
            .fleet_id = owner.fleet_id,
            .event_id = owner.event_id,
            .repository = repository,
            .branch = branch,
            .pr_number = number,
            .pr_url = html_url,
        },
        .merged_commit_sha = sha,
        .merged_at = merged_at,
    }) catch return dbFailure(hx);
    if (arrival.outcome == .recorded or arrival.outcome == .same) {
        log.info("repair_pr_merge_recorded", .{ .fleet_id = owner.fleet_id, .event_id = owner.event_id, .pr_number = number, .commit = sha[0..@min(sha.len, 12)] });
        hx.ok(.ok, .{ .merged = sha });
    } else {
        hx.ok(.ok, .{ .ignored = IGNORED_UNMERGED });
    }
    return .handled;
}

fn handleWorkflowRun(hx: Hx, conn: *pg.Conn, root: std.json.ObjectMap, owner: *const gate_resolve.Owner, repository: []const u8, branch: []const u8) Interception {
    const action = stringField(root, FIELD_ACTION) orelse return malformed(hx);
    if (!std.mem.eql(u8, action, ACTION_COMPLETED)) return ignored(hx, IGNORED_REPAIR_BRANCH);
    const run = objectField(root, FIELD_WORKFLOW_RUN) orelse return malformed(hx);
    const provider_run_id = intField(run, FIELD_ID) orelse return malformed(hx);
    const workflow_name = nonEmpty(stringField(run, FIELD_NAME)) orelse return malformed(hx);
    const head_commit_sha = nonEmpty(stringField(run, FIELD_HEAD_SHA)) orelse return malformed(hx);
    const conclusion = nonEmpty(stringField(run, FIELD_CONCLUSION)) orelse return malformed(hx);
    const completed_text = nonEmpty(stringField(run, FIELD_UPDATED_AT)) orelse return malformed(hx);
    const completed_at = providerTimestamp(completed_text) orelse return malformed(hx);
    const outcome = repair_evidence.recordRun(hx.alloc, conn, .{
        .workspace_id = owner.workspace_id,
        .fleet_id = owner.fleet_id,
        .event_id = owner.event_id,
        .repository = repository,
        .branch = branch,
        .workflow_name = workflow_name,
        .provider_run_id = provider_run_id,
        .head_commit_sha = head_commit_sha,
        .conclusion = conclusion,
        .completed_at = completed_at,
    }) catch return dbFailure(hx);
    const status = if (outcome == .inserted) RECORDED_REPAIR_RUN else REPLAYED_REPAIR_RUN;
    if (outcome == .inserted) {
        log.info(RECORDED_REPAIR_RUN, .{ .fleet_id = owner.fleet_id, .event_id = owner.event_id, .repository = repository, .provider_run_id = provider_run_id });
    } else {
        log.info(REPLAYED_REPAIR_RUN, .{ .fleet_id = owner.fleet_id, .event_id = owner.event_id, .repository = repository, .provider_run_id = provider_run_id });
    }
    hx.ok(.ok, .{ .status = status });
    return .handled;
}

fn branchForEvent(event: []const u8, root: std.json.ObjectMap) ?[]const u8 {
    if (std.mem.eql(u8, event, filter.EVENT_PULL_REQUEST)) {
        const pull_request = objectField(root, FIELD_PULL_REQUEST) orelse return null;
        const head = objectField(pull_request, FIELD_HEAD) orelse return null;
        return stringField(head, FIELD_REF);
    }
    if (std.mem.eql(u8, event, filter.EVENT_WORKFLOW_RUN)) {
        const run = objectField(root, FIELD_WORKFLOW_RUN) orelse return null;
        return stringField(run, FIELD_HEAD_BRANCH);
    }
    return null;
}

fn repositoryName(root: std.json.ObjectMap) ?[]const u8 {
    const repository = objectField(root, FIELD_REPOSITORY) orelse return null;
    return stringField(repository, FIELD_FULL_NAME);
}

fn providerTimestamp(text: []const u8) ?i64 {
    if (text.len != 20 or text[text.len - 1] != 'Z') return null;
    return event_time.parseSince(text, 0) catch null;
}

fn nonEmpty(value: ?[]const u8) ?[]const u8 {
    const text = value orelse return null;
    return if (text.len == 0) null else text;
}

fn objectField(object: std.json.ObjectMap, key: []const u8) ?std.json.ObjectMap {
    return switch (object.get(key) orelse return null) {
        .object => |value| value,
        else => null,
    };
}

fn stringField(object: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    return switch (object.get(key) orelse return null) {
        .string => |value| value,
        else => null,
    };
}

fn intField(object: std.json.ObjectMap, key: []const u8) ?i64 {
    return switch (object.get(key) orelse return null) {
        .integer => |value| value,
        else => null,
    };
}

fn boolField(object: std.json.ObjectMap, key: []const u8) ?bool {
    return switch (object.get(key) orelse return null) {
        .bool => |value| value,
        else => null,
    };
}

fn ignored(hx: Hx, reason: []const u8) Interception {
    hx.ok(.ok, .{ .ignored = reason });
    return .handled;
}

fn malformed(hx: Hx) Interception {
    hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MALFORMED_JSON);
    return .handled;
}

fn dbUnavailable(hx: Hx) Interception {
    common.internalDbUnavailable(hx.res, hx.req_id);
    return .handled;
}

fn dbFailure(hx: Hx) Interception {
    common.internalDbError(hx.res, hx.req_id);
    return .handled;
}
