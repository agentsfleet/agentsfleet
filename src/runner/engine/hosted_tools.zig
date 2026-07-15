//! Hosted-tool defaults for agentsfleet-runner.
//!
//! NullClaw's local scheduler tools are valid for standalone NullClaw, but not
//! for hosted agentsfleet runs. Hosted recurrence is owned by agentsfleetd cron
//! plus Upstash QStash, so the runner must filter local scheduler tools even on
//! the `tools: null` fallback path.

const std = @import("std");
const nullclaw = @import("nullclaw");

const Config = nullclaw.config.Config;
const tools_mod = nullclaw.tools;
const tool_bridge = @import("tool_bridge.zig");

/// Build the default hosted tool set from NullClaw and drop local scheduler
/// tools before exposing them to a fleet child.
pub fn buildDefault(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    cfg: *const Config,
) ![]tools_mod.Tool {
    const tools = try tools_mod.allTools(alloc, workspace_path, .{
        .allowed_paths = &.{workspace_path},
        .tools_config = cfg.tools,
    });
    return filterUnsupported(alloc, tools);
}

fn filterUnsupported(
    alloc: std.mem.Allocator,
    tools: []tools_mod.Tool,
) ![]tools_mod.Tool {
    var original_owned = true;
    errdefer if (original_owned) {
        for (tools) |t| t.deinit(alloc);
        alloc.free(tools);
    };

    var filtered: std.ArrayList(tools_mod.Tool) = .empty;
    errdefer {
        for (filtered.items) |t| t.deinit(alloc);
        filtered.deinit(alloc);
    }

    try filtered.ensureTotalCapacity(alloc, tools.len);
    for (tools) |t| {
        if (tool_bridge.isUnsupportedHostedToolName(t.name())) {
            t.deinit(alloc);
            continue;
        }
        filtered.appendAssumeCapacity(t);
    }

    alloc.free(tools);
    original_owned = false;
    return filtered.toOwnedSlice(alloc);
}
