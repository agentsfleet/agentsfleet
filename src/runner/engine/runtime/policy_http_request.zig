//! Policy-aware `http_request` tool. Replaces NullClaw's plain
//! `HttpRequestTool` for sessions whose `ExecutionPolicy` carries a
//! per-execution network allowlist + resolved `secrets_map`.
//!
//! Order of operations on every tool call (load-bearing — pinned by tests):
//!   1. Refuse mintable credentials in the URL. Repair sessions also refuse
//!      credential placeholders in bodies, URLs, and non-Authorization
//!      headers before substitution or minting. Then substitute permitted
//!      `${secrets.NAME.FIELD}` placeholders against `secrets_map`.
//!      Substitution fails closed (the agent sees the error and reformulates).
//!   2. Defence-in-depth: refuse to dispatch if the substituted bytes still
//!      contain `${secrets.` anywhere — partial substitution is a leak vector.
//!   3. Extract the host from the substituted url and check it against
//!      `policy.network_policy.allow`. Off-list hosts return
//!      `host_not_allowed` without reflecting resolved credential bytes.
//!   4. Apply daemon-authored generic request boundaries, when present.
//!   5. Delegate to NullClaw's `HttpRequestTool` with the substituted
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
const credential_placement = @import("credential_placement.zig");
const request_args = @import("request_args.zig");
const context_budget = @import("../context_budget.zig");
const credential_request = @import("../credential_request.zig");
const http_request_policy = @import("http_request_policy.zig");

const Self = @This();

const S_SUBSTITUTION_LEFT_PLACEHOLDER = "substitution_left_placeholder";
const S_METHOD_NOT_ALLOWED = "method_not_allowed";
const S_CREDENTIAL_HOST_NOT_ALLOWED = "credential_host_not_allowed";
const S_CREDENTIAL_PLACEMENT_NOT_ALLOWED = "credential_placement_not_allowed";
const S_REQUEST_POLICY_NOT_ALLOWED = "request_policy_not_allowed";
const S_HOST_NOT_ALLOWED = "host_not_allowed";
const S_GET = "GET";
const S_HEAD = "HEAD";
const S_POST = "POST";

const InnerExecute = *const fn (*HttpRequestTool, std.mem.Allocator, JsonObjectMap) anyerror!ToolResult;

/// Borrowed pointer to the session-owned policy. The session arena
/// outlives this tool — the tool is freed at stage end, the session
/// at destroy_execution — so the borrow is safe for every call.
policy: *const context_budget.ExecutionPolicy,
inner: HttpRequestTool,
/// The child→runner on-demand mint channel, or null when no session wired one
/// (register-only/test path). A mintable `${secrets.<id>.token}` mints through
/// this at dispatch; static placeholders never touch it. Borrowed for the call.
cred_channel: ?credential_request.Channel = null,
inner_execute: InnerExecute = dispatchInner,

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

    const url_val = args.get(request_args.ARG_URL) orelse return ToolResult.fail("Missing 'url' parameter");
    const url_str = switch (url_val) {
        .string => |s| s,
        else => return ToolResult.fail("Invalid 'url' parameter"),
    };
    if (credential_placement.mintableCredentialInUrl(self.policy, url_str))
        return ToolResult.fail(S_CREDENTIAL_HOST_NOT_ALLOWED);
    if (!credential_placement.requestAllowed(self.policy, args))
        return ToolResult.fail(S_CREDENTIAL_PLACEMENT_NOT_ALLOWED);

    // One resolver per tool call: its cache dedups repeated mintable placeholders
    // across url ∪ headers ∪ body (the broker caches across calls). Arena-scoped —
    // minted token bytes die with this call (VLT), never an alias into the policy.
    var resolver = credential_request.MintResolver{
        .mintable = self.policy.mintable,
        .channel = self.cred_channel,
    };

    const subst_url = try substOrFail(arena, url_str, self.policy.secrets_map, &resolver);
    if (!secret_substitution.assertNoLeftover(subst_url))
        return ToolResult.fail(S_SUBSTITUTION_LEFT_PLACEHOLDER);

    const host = extractHost(subst_url) orelse
        return ToolResult.fail("Invalid URL: cannot extract host");
    if (!hostInAllowlist(host, self.policy.network_policy.allow))
        return ToolResult.fail(S_HOST_NOT_ALLOWED);
    if (!credential_placement.credentialsBoundToHost(self.policy, host, args))
        return ToolResult.fail(S_CREDENTIAL_HOST_NOT_ALLOWED);

    const subst_args = request_args.build(
        arena,
        args,
        subst_url,
        self.policy.secrets_map,
        &resolver,
    ) catch |err| switch (err) {
        error.Leftover => return ToolResult.fail(S_SUBSTITUTION_LEFT_PLACEHOLDER),
        error.SubstFailed => return error.SubstFailed,
        error.OutOfMemory => return error.OutOfMemory,
    };

    const method = requestMethod(subst_args) orelse return ToolResult.fail(S_METHOD_NOT_ALLOWED);
    const verdict = http_request_policy.validate(
        arena,
        self.policy.http_origin_policies,
        subst_url,
        method,
        subst_args.get(request_args.ARG_BODY),
    );
    if (verdict == .denied) return ToolResult.fail(S_REQUEST_POLICY_NOT_ALLOWED);
    if (!readOnlyRequestAllowed(self.policy, subst_url, method, verdict))
        return ToolResult.fail(S_METHOD_NOT_ALLOWED);
    return self.inner_execute(&self.inner, allocator, subst_args);
}

fn dispatchInner(inner: *HttpRequestTool, allocator: std.mem.Allocator, args: JsonObjectMap) anyerror!ToolResult {
    return inner.execute(allocator, args);
}

/// Run secret substitution into the per-call arena. Substitution errors
/// (missing secret, malformed placeholder, non-string field) collapse
/// into a single `SubstFailed` so the call site can reject without
/// leaking the structured cause through the tool result. The agent
/// retries with a different placeholder; the failure detail lands in
/// the runner log via the catch site.
fn substOrFail(
    arena: std.mem.Allocator,
    raw: []const u8,
    secrets_map: ?std.json.Value,
    resolver: ?*credential_request.MintResolver,
) error{SubstFailed}![]u8 {
    return secret_substitution.substitute(arena, raw, secrets_map, resolver) catch
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

fn readOnlyRequestAllowed(
    policy: *const context_budget.ExecutionPolicy,
    url: []const u8,
    method: []const u8,
    scoped_verdict: http_request_policy.Verdict,
) bool {
    if (!policy.network_policy.read_only) return true;
    if (std.ascii.eqlIgnoreCase(method, S_GET) or std.ascii.eqlIgnoreCase(method, S_HEAD)) return true;
    if (!std.ascii.eqlIgnoreCase(method, S_POST)) return false;
    if (scoped_verdict == .allowed) return true;
    for (policy.network_policy.read_post_paths) |prefix| {
        if (urlHasAllowedPrefix(url, prefix)) return true;
    }
    return false;
}

fn requestMethod(args: JsonObjectMap) ?[]const u8 {
    const value = args.get(request_args.ARG_METHOD) orelse return S_GET;
    return if (value == .string) value.string else null;
}

fn urlHasAllowedPrefix(url: []const u8, prefix: []const u8) bool {
    if (!std.mem.startsWith(u8, url, prefix)) return false;
    if (url.len == prefix.len) return true;
    return url[prefix.len] == '?';
}

// The suite lives in `policy_http_request_test.zig` (sibling, keeps this
// file under the 350-line cap); pull it into the test build here.
test {
    _ = @import("policy_http_request_test.zig");
    _ = @import("policy_http_read_only_test.zig");
    _ = @import("http_request_policy_test.zig");
}
