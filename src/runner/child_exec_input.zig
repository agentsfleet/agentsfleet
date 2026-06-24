//! Engine-input assembly for the `__execute` child — turns a `LeasePayload`
//! into the fleet_config / tools_spec / message args and the installed-
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
    /// NON-OWNING view of `fleet_obj` (the same backing map) — never `deinit`
    /// this; it is freed only via `fleet_obj`. Null when the policy contributed
    /// no fleet-config keys.
    fleet_config: ?std.json.Value,
    /// NON-OWNING view of `tools_arr` — never `deinit` this; freed via `tools_arr`.
    tools_spec: ?std.json.Value,
    /// Borrows either `req_parsed`'s arena (the parsed-string path) or
    /// `payload.event.request_json` (every fallback path). Path-dependent, so it
    /// is valid only until `deinit` — never use it after the struct is freed.
    message: ?[]const u8,
    /// Owns the `fleet_config` backing map.
    fleet_obj: std.json.ObjectMap,
    /// Owns the `tools_spec` backing array.
    tools_arr: std.json.Array,
    req_parsed: ?std.json.Parsed(std.json.Value),

    pub fn deinit(self: CallArgs, alloc: std.mem.Allocator) void {
        var a = self.fleet_obj;
        a.deinit(alloc);
        var t = self.tools_arr;
        t.deinit();
        if (self.req_parsed) |p| p.deinit();
    }
};

/// Build engine args from the leased policy + event. Fleet-config keys reuse
/// the `wire` constants the engine reads them back with (RULE UFS). Fails closed
/// on allocation failure: a partial `fleet_config` never escapes — the caller
/// reports a startup-posture failure instead of invoking the model with a
/// half-built config (e.g. a provider with no key). Caller owns the returned
/// value (deinit with the same allocator).
pub fn buildCallArgs(alloc: std.mem.Allocator, payload: LeasePayload) error{OutOfMemory}!CallArgs {
    var fleet_obj: std.json.ObjectMap = .empty;
    errdefer fleet_obj.deinit(alloc);
    if (payload.policy.context.model.len > 0)
        try fleet_obj.put(alloc, wire.model, .{ .string = payload.policy.context.model });
    // Provider + key are the authoritative resolved values delivered on the
    // lease (the key the tenant is billed for) — atomic: the resolver always
    // produces both or neither. A half-populated pair is a malformed lease; we
    // inject nothing so the engine fails to authenticate cleanly rather than
    // running against the wrong provider. The pair is atomic under OOM too: if
    // the api_key `put` fails, `try` unwinds the whole build (the errdefer frees
    // the already-inserted provider) so a provider-without-key never reaches the
    // engine. `secrets_map` carries tool credentials only — a tool secret named
    // "llm" is NOT the provider key.
    if (payload.policy.provider.len > 0 and payload.policy.api_key.len > 0) {
        try fleet_obj.put(alloc, wire.provider, .{ .string = payload.policy.provider });
        try fleet_obj.put(alloc, wire.api_key, .{ .string = payload.policy.api_key });
        // Custom OpenAI-compatible endpoint: carry the dialed URL so the runner
        // sets it on the nullclaw provider entry (the provider name is already
        // `custom:<url>`). Only present for custom endpoints — a named provider's
        // base_url is null and this key is omitted, leaving the named path intact.
        if (payload.policy.base_url) |url| {
            if (url.len > 0) try fleet_obj.put(alloc, wire.base_url, .{ .string = url });
        }
    } else if (payload.policy.provider.len > 0 or payload.policy.api_key.len > 0) {
        log.warn("fleet_provider_key_incomplete", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .has_provider = payload.policy.provider.len > 0, .fleet_id = payload.event.fleet_id });
    }

    var tools_arr = std.json.Array.init(alloc);
    errdefer tools_arr.deinit();
    for (payload.policy.tools) |name|
        try tools_arr.append(.{ .string = name });

    // Malformed request JSON falls back to the raw body as the message (a defined,
    // safe behaviour) — but an OOM during the parse fails closed like any other
    // allocation failure, rather than silently degrading to the raw body under
    // memory pressure.
    const req_parsed: ?std.json.Parsed(std.json.Value) =
        std.json.parseFromSlice(std.json.Value, alloc, payload.event.request_json, .{}) catch |err| switch (err) {
            error.OutOfMemory => return error.OutOfMemory,
            else => null,
        };

    const message: ?[]const u8 = blk: {
        const pv = if (req_parsed) |p| p.value else break :blk payload.event.request_json;
        if (pv != .object) break :blk payload.event.request_json;
        const mv = pv.object.get(wire.message) orelse break :blk payload.event.request_json;
        if (mv != .string) break :blk payload.event.request_json;
        break :blk mv.string;
    };

    return .{
        .fleet_config = if (fleet_obj.count() > 0) .{ .object = fleet_obj } else null,
        .tools_spec = if (tools_arr.items.len > 0) .{ .array = tools_arr } else null,
        .message = message,
        .fleet_obj = fleet_obj,
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

test {
    _ = @import("child_exec_input_test.zig");
}
