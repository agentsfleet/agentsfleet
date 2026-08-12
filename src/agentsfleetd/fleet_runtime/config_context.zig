//! Parser for the optional runtime context-budget block.

const std = @import("std");
const config_types = @import("config_types.zig");

const FleetConfigError = config_types.FleetConfigError;
const FleetContextBudget = config_types.FleetContextBudget;

const S_CONTEXT_CAP_TOKENS = "context_cap_tokens";
const S_MEMORY_CHECKPOINT_EVERY = "memory_checkpoint_every";
const S_STAGE_CHUNK_THRESHOLD = "stage_chunk_threshold";
const S_TOOL_WINDOW = "tool_window";

pub fn parse(runtime: std.json.ObjectMap, context_key: []const u8) FleetConfigError!?FleetContextBudget {
    const value = runtime.get(context_key) orelse return null;
    const object = switch (value) {
        .object => |item| item,
        else => return FleetConfigError.InvalidFieldType,
    };
    try ensureKnownKeys(object);
    return FleetContextBudget{
        .context_cap_tokens = try readU32(object, S_CONTEXT_CAP_TOKENS),
        .tool_window = try readU32(object, S_TOOL_WINDOW),
        .memory_checkpoint_every = try readU32(object, S_MEMORY_CHECKPOINT_EVERY),
        .stage_chunk_threshold = try readF32(object, S_STAGE_CHUNK_THRESHOLD),
    };
}

fn ensureKnownKeys(context: std.json.ObjectMap) FleetConfigError!void {
    const known = [_][]const u8{
        S_CONTEXT_CAP_TOKENS,
        S_TOOL_WINDOW,
        S_MEMORY_CHECKPOINT_EVERY,
        S_STAGE_CHUNK_THRESHOLD,
    };
    var fields = context.iterator();
    while (fields.next()) |entry| {
        var found = false;
        for (known) |key| {
            if (!std.mem.eql(u8, key, entry.key_ptr.*)) continue;
            found = true;
            break;
        }
        if (!found) return FleetConfigError.UnknownRuntimeKey;
    }
}

fn readU32(object: std.json.ObjectMap, key: []const u8) FleetConfigError!u32 {
    const value = object.get(key) orelse return 0;
    return switch (value) {
        .integer => |integer| blk: {
            if (integer < 0 or integer > std.math.maxInt(u32)) return FleetConfigError.InvalidFieldType;
            break :blk @intCast(integer);
        },
        .string => |string| if (std.mem.eql(u8, string, "auto")) 0 else return FleetConfigError.InvalidFieldType,
        else => return FleetConfigError.InvalidFieldType,
    };
}

fn readF32(object: std.json.ObjectMap, key: []const u8) FleetConfigError!f32 {
    const value = object.get(key) orelse return 0.0;
    return switch (value) {
        .float => |float| @floatCast(float),
        .integer => |integer| @floatFromInt(integer),
        else => return FleetConfigError.InvalidFieldType,
    };
}
