//! Engine-input assembly for the `__execute` child — turns a `LeasePayload`
//! into the agent_config / tools_spec / message args and the installed-
//! instructions reasoning context. Split from `child_exec.zig` to keep that
//! file under the RULE FLL line limit; consumed only by `child_exec.runEngine`.

const std = @import("std");
const logging = @import("log");
const contract = @import("contract");

const wire = @import("engine/wire.zig");
const client_errors = @import("engine/client_errors.zig");

const log = logging.scoped(.runner_exec);
const ERR_EXEC_RUNNER_INVALID_CONFIG = client_errors.ERR_EXEC_RUNNER_INVALID_CONFIG;
const LeasePayload = contract.protocol.LeasePayload;

/// Engine-call args resolved from the lease. `deinit` releases the two JSON
/// containers (caller-owned allocator pattern).
pub const CallArgs = struct {
    agent_config: ?std.json.Value,
    tools_spec: ?std.json.Value,
    message: ?[]const u8,
    agent_obj: std.json.ObjectMap,
    tools_arr: std.json.Array,
    req_parsed: ?std.json.Parsed(std.json.Value),

    pub fn deinit(self: CallArgs, alloc: std.mem.Allocator) void {
        var a = self.agent_obj;
        a.deinit(alloc);
        var t = self.tools_arr;
        t.deinit();
        if (self.req_parsed) |p| p.deinit();
    }
};

/// Build engine args from the leased policy + event. Agent-config keys reuse
/// the `wire` constants the engine reads them back with (RULE UFS).
pub fn buildCallArgs(alloc: std.mem.Allocator, payload: LeasePayload) CallArgs {
    var agent_obj: std.json.ObjectMap = .empty;
    if (payload.policy.context.model.len > 0)
        agent_obj.put(alloc, wire.model, .{ .string = payload.policy.context.model }) catch |err| log.warn("agent_model_arg_dropped", .{ .err = @errorName(err) });
    // Provider + key are the authoritative resolved values delivered on the
    // lease (the key the tenant is billed for) — atomic: the resolver always
    // produces both or neither. A half-populated pair is a malformed lease; we
    // inject nothing so the engine fails to authenticate cleanly rather than
    // running against the wrong provider. `secrets_map` carries tool credentials
    // only — a tool secret named "llm" is NOT the provider key.
    if (payload.policy.provider.len > 0 and payload.policy.api_key.len > 0) {
        agent_obj.put(alloc, wire.provider, .{ .string = payload.policy.provider }) catch |err| log.warn("agent_provider_arg_dropped", .{ .err = @errorName(err) });
        agent_obj.put(alloc, wire.api_key, .{ .string = payload.policy.api_key }) catch |err| log.warn("agent_apikey_arg_dropped", .{ .err = @errorName(err) });
    } else if (payload.policy.provider.len > 0 or payload.policy.api_key.len > 0) {
        log.warn("agent_provider_key_incomplete", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .has_provider = payload.policy.provider.len > 0 });
    }

    var tools_arr = std.json.Array.init(alloc);
    for (payload.policy.tools) |name|
        tools_arr.append(.{ .string = name }) catch |err| log.warn("agent_tool_arg_dropped", .{ .err = @errorName(err) });

    const req_parsed: ?std.json.Parsed(std.json.Value) =
        std.json.parseFromSlice(std.json.Value, alloc, payload.event.request_json, .{}) catch null;

    const message: ?[]const u8 = blk: {
        const pv = if (req_parsed) |p| p.value else break :blk payload.event.request_json;
        if (pv != .object) break :blk payload.event.request_json;
        const mv = pv.object.get(wire.message) orelse break :blk payload.event.request_json;
        if (mv != .string) break :blk payload.event.request_json;
        break :blk mv.string;
    };

    return .{
        .agent_config = if (agent_obj.count() > 0) .{ .object = agent_obj } else null,
        .tools_spec = if (tools_arr.items.len > 0) .{ .array = tools_arr } else null,
        .message = message,
        .agent_obj = agent_obj,
        .tools_arr = tools_arr,
        .req_parsed = req_parsed,
    };
}

/// Build the reasoning context carrying the installed `SKILL.md` body. Caller
/// owns the returned map (deinit with the same allocator). Errors only on
/// allocation failure; the caller fails closed (never runs a generic turn).
pub fn buildInstructionsContext(alloc: std.mem.Allocator, instructions: []const u8) !std.json.ObjectMap {
    var ctx_obj: std.json.ObjectMap = .empty;
    errdefer ctx_obj.deinit(alloc);
    try ctx_obj.put(alloc, wire.installed_instructions, .{ .string = instructions });
    return ctx_obj;
}
