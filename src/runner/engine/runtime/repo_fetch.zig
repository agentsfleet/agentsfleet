//! The sandboxed child's `repo_fetch` tool (M157 §4) — the only way a fleet can
//! obtain a working tree, and the last link in the fetch chain.
//!
//! The child holds no token, no route out of its network namespace, and no
//! control-plane URL, so it cannot clone anything itself. It asks its runner:
//! this tool frames a `repo_fetch_request` up stdout and blocks for the reply,
//! exactly as a mintable `${secrets.NAME.token}` blocks for its
//! `credential_response`. The daemon validates the ask against the fleet's
//! declared `repositories` binding, mints, fetches depth-bounded into the
//! workspace it derives from `lease_id`, and answers with a relative path.
//!
//! WHAT THE MODEL CONTROLS AND WHAT IT DOES NOT. It names the repository, the
//! commit, and the branch — nobody else knows which repair is being proposed. It
//! does not name a workspace, a path, or a credential: those are the daemon's,
//! and there is nothing in the ask for a prompt-injected child to forge. So the
//! worst a talked-into-it model achieves here is a refusal it then has to read.
//!
//! The result carries a path and never a token (Invariant 9), so what lands in
//! the model's context after a successful fetch is the single word `repo`.

const std = @import("std");
const nullclaw = @import("nullclaw");
const tools_mod = nullclaw.tools;
const Tool = tools_mod.Tool;
const ToolResult = tools_mod.ToolResult;
const JsonObjectMap = tools_mod.JsonObjectMap;

const fetch_request = @import("../repo_fetch_request.zig");

const Self = @This();

/// The child→runner fetch channel, or null when no session wired one (the
/// register-only / unit-test path). Null fails the call closed: a fleet with no
/// channel gets a refusal, never a silent success against an absent tree.
channel: ?fetch_request.Channel = null,

pub const tool_name = "repo_fetch";
pub const tool_description =
    "Fetch a repository at a specific commit into this run's workspace, so a " ++
    "revert can be computed against a real working tree. The runner performs " ++
    "the fetch: it validates the repository against the fleet's declared " ++
    "binding and refuses anything outside it. Returns the workspace-relative " ++
    "path to the checked-out tree. No credential is exposed to the run.";
pub const tool_params =
    \\{"type":"object","properties":{"repository":{"type":"string","description":"Full owner/repo, e.g. acme/payments. Must be one the fleet declared."},"commit":{"type":"string","description":"Full lowercase object id of the suspect commit to revert. Abbreviated ids, branches, and tags are refused."},"head":{"type":"string","description":"Branch the revert targets. Omit for the remote's default head."}},"required":["repository","commit"]}
;

const ARG_REPOSITORY = "repository";
const ARG_COMMIT = "commit";
const ARG_HEAD = "head";

/// Cap on the refusal reason accumulated from the daemon. Every real reason is a
/// short named string; this bounds a wire-skewed reply, not a legitimate one.
const MAX_REASON_BYTES: usize = 512;

const vtable = tools_mod.ToolVTable(@This());

pub fn tool(self: *Self) Tool {
    return .{ .ptr = @ptrCast(self), .vtable = &vtable };
}

pub fn execute(self: *Self, allocator: std.mem.Allocator, args: JsonObjectMap) !ToolResult {
    const channel = self.channel orelse return ToolResult.fail(S_FETCH_UNAVAILABLE);

    const repository = stringArg(args, ARG_REPOSITORY) orelse return ToolResult.fail(S_MISSING_REPOSITORY);
    const commit = stringArg(args, ARG_COMMIT) orelse return ToolResult.fail(S_MISSING_COMMIT);
    // Absent and empty mean the same thing — the remote's default head — so a
    // model that passes `""` gets the documented behaviour rather than a refusal.
    const head = stringArg(args, ARG_HEAD) orelse "";

    var arena_state = std.heap.ArenaAllocator.init(allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    var reason: std.ArrayList(u8) = .empty;
    const path = fetch_request.request(channel, arena, .{
        .repository = repository,
        .commit = commit,
        .head = head,
    }, &reason) catch |err| return failure(allocator, err, reason.items);

    // Duped out of the arena: `ToolResult.output` outlives this frame and the
    // engine frees it with `allocator`.
    return .{ .success = true, .output = try allocator.dupe(u8, path) };
}

/// Map a typed round-trip failure onto something the model can act on. A daemon
/// refusal carries its own reason and is reported verbatim; every transport
/// failure gets a fixed string, because a wire fault is not something the model
/// can reformulate around and inventing detail would only invite it to try.
fn failure(allocator: std.mem.Allocator, err: fetch_request.FetchError, reason: []const u8) !ToolResult {
    if (err != error.FetchRefused) return ToolResult.fail(switch (err) {
        error.FetchTimeout => S_FETCH_TIMEOUT,
        error.OutOfMemory => S_FETCH_OOM,
        else => S_FETCH_TRANSPORT,
    });
    if (reason.len == 0) return ToolResult.fail(S_FETCH_REFUSED);
    const capped = reason[0..@min(reason.len, MAX_REASON_BYTES)];
    const msg = try std.fmt.allocPrint(allocator, "{s}: {s}", .{ S_FETCH_REFUSED, capped });
    return .{ .success = false, .output = "", .error_msg = msg };
}

fn stringArg(args: JsonObjectMap, name: []const u8) ?[]const u8 {
    const val = args.get(name) orelse return null;
    return switch (val) {
        .string => |s| s,
        else => null,
    };
}

// Wire words the model reads and reformulates against — named so they are
// greppable and identical everywhere they appear (RULE UFS).
const S_FETCH_UNAVAILABLE = "repo_fetch is not available to this run";
const S_MISSING_REPOSITORY = "Missing 'repository' parameter";
const S_MISSING_COMMIT = "Missing 'commit' parameter";
const S_FETCH_REFUSED = "repo_fetch refused";
const S_FETCH_TIMEOUT = "repo_fetch timed out against the lease deadline";
const S_FETCH_TRANSPORT = "repo_fetch lost its channel to the runner";
const S_FETCH_OOM = "repo_fetch could not allocate";

test {
    _ = @import("repo_fetch_tool_test.zig");
}
