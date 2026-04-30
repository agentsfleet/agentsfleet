//! Policy-aware `http_request` tool. Replaces NullClaw's plain
//! `HttpRequestTool` for sessions whose `ExecutionPolicy` carries a
//! per-execution network allowlist + resolved `secrets_map`.
//!
//! Order of operations on every tool call (load-bearing — pinned by tests):
//!   1. Substitute `${secrets.NAME.FIELD}` placeholders in url, body, and
//!      every header value, against the session's `secrets_map`.
//!      Substitution fails closed (the agent sees the error and reformulates).
//!   2. Defence-in-depth: refuse to dispatch if the substituted bytes still
//!      contain `${secrets.` anywhere — partial substitution is a leak vector.
//!   3. Extract the host from the substituted url and check it against
//!      `policy.network_policy.allow`. Off-list hosts return
//!      `host_not_allowed: <host>` so the agent reasons about the failure.
//!   4. Delegate to NullClaw's `HttpRequestTool` with the substituted
//!      args. The inner tool owns curl, SSRF protection, response parsing.
//!
//! The agent's frame log (the redacted view that flows back into context
//! via `runner_progress.Adapter`) only ever sees the original args —
//! placeholder bytes, never the resolved values. The substituted bytes
//! are arena-scoped and freed before this function returns.

const std = @import("std");
const nullclaw = @import("nullclaw");
const tools_mod = nullclaw.tools;
const Tool = tools_mod.Tool;
const ToolResult = tools_mod.ToolResult;
const JsonObjectMap = tools_mod.JsonObjectMap;
const HttpRequestTool = tools_mod.http_request.HttpRequestTool;

const secret_substitution = @import("secret_substitution.zig");
const context_budget = @import("../context_budget.zig");

const Self = @This();

const arg_url: []const u8 = "url";
const arg_method: []const u8 = "method";
const arg_headers: []const u8 = "headers";
const arg_body: []const u8 = "body";

/// Borrowed pointer to the session-owned policy. The session arena
/// outlives this tool — the tool is freed at stage end, the session
/// at destroy_execution — so the borrow is safe for every call.
policy: *const context_budget.ExecutionPolicy,
inner: HttpRequestTool,

pub const tool_name = HttpRequestTool.tool_name;
pub const tool_description = HttpRequestTool.tool_description;
pub const tool_params = HttpRequestTool.tool_params;

const vtable = tools_mod.ToolVTable(@This());

pub fn tool(self: *Self) Tool {
    return .{ .ptr = @ptrCast(self), .vtable = &vtable };
}

pub fn execute(self: *Self, allocator: std.mem.Allocator, args: JsonObjectMap) !ToolResult {
    var arena_state = std.heap.ArenaAllocator.init(allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const url_val = args.get(arg_url) orelse return ToolResult.fail("Missing 'url' parameter");
    const url_str = switch (url_val) {
        .string => |s| s,
        else => return ToolResult.fail("Invalid 'url' parameter"),
    };

    const subst_url = substOrFail(arena, url_str, self.policy.secrets_map) catch |fail|
        return fail;
    if (!secret_substitution.assertNoLeftover(subst_url))
        return ToolResult.fail("substitution_left_placeholder");

    const host = extractHost(subst_url) orelse
        return ToolResult.fail("Invalid URL: cannot extract host");
    if (!hostInAllowlist(host, self.policy.network_policy.allow)) {
        const msg = try std.fmt.allocPrint(allocator, "host_not_allowed: {s}", .{host});
        return ToolResult{ .success = false, .output = "", .error_msg = msg };
    }

    var subst_args = JsonObjectMap.init(arena);
    try subst_args.put(arg_url, .{ .string = subst_url });
    if (args.get(arg_method)) |m| try subst_args.put(arg_method, m);

    if (args.get(arg_headers)) |hv| {
        if (hv == .object) {
            var subst_headers = JsonObjectMap.init(arena);
            var it = hv.object.iterator();
            while (it.next()) |e| {
                const v = e.value_ptr.*;
                const replaced: std.json.Value = switch (v) {
                    .string => |s| blk: {
                        const r = substOrFail(arena, s, self.policy.secrets_map) catch |fail|
                            return fail;
                        if (!secret_substitution.assertNoLeftover(r))
                            return ToolResult.fail("substitution_left_placeholder");
                        break :blk .{ .string = r };
                    },
                    else => v,
                };
                try subst_headers.put(e.key_ptr.*, replaced);
            }
            try subst_args.put(arg_headers, .{ .object = subst_headers });
        } else {
            try subst_args.put(arg_headers, hv);
        }
    }

    if (args.get(arg_body)) |bv| {
        const replaced: std.json.Value = switch (bv) {
            .string => |s| blk: {
                const r = substOrFail(arena, s, self.policy.secrets_map) catch |fail|
                    return fail;
                if (!secret_substitution.assertNoLeftover(r))
                    return ToolResult.fail("substitution_left_placeholder");
                break :blk .{ .string = r };
            },
            else => bv,
        };
        try subst_args.put(arg_body, replaced);
    }

    return self.inner.execute(allocator, subst_args);
}

/// Run secret substitution into the per-call arena. Substitution errors
/// (missing secret, malformed placeholder, non-string field) collapse
/// into a single `SubstFailed` so the call site can reject without
/// leaking the structured cause through the tool result. The agent
/// retries with a different placeholder; the failure detail lands in
/// the executor log via the catch site.
fn substOrFail(
    arena: std.mem.Allocator,
    raw: []const u8,
    secrets_map: ?std.json.Value,
) error{SubstFailed}![]u8 {
    return secret_substitution.substitute(arena, raw, secrets_map) catch
        return error.SubstFailed;
}

fn extractHost(url: []const u8) ?[]const u8 {
    const uri = std.Uri.parse(url) catch return null;
    return switch (uri.host orelse return null) {
        .raw, .percent_encoded => |s| s,
    };
}

fn hostInAllowlist(host: []const u8, allow: []const []const u8) bool {
    for (allow) |entry| {
        if (std.ascii.eqlIgnoreCase(host, entry)) return true;
    }
    return false;
}

// Tests live in `policy_http_request_test.zig` (sibling) to keep this file
// under the 350-line cap; the file-as-struct boundary stays here.
