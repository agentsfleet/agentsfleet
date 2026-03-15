const std = @import("std");
const nullclaw = @import("nullclaw");

const Config = nullclaw.config.Config;
const Agent = nullclaw.agent.Agent;
const providers = nullclaw.providers;
const tools_mod = nullclaw.tools;
const memory_mod = nullclaw.memory;
const observability = nullclaw.observability;

const log = std.log.scoped(.agents);

pub const AgentResult = struct {
    content: []const u8,
    token_count: u64,
    wall_seconds: u64,
    exit_ok: bool,
};

pub const ObserverBackend = enum {
    log,
    noop,
    verbose,
};

const ObserverRuntime = struct {
    backend: ObserverBackend,
    noop: observability.NoopObserver = .{},
    log_observer: observability.LogObserver = .{},
    verbose_observer: observability.VerboseObserver = .{},

    fn init(alloc: std.mem.Allocator) ObserverRuntime {
        return .{ .backend = configuredObserverBackend(alloc) };
    }

    fn observer(self: *ObserverRuntime) observability.Observer {
        return switch (self.backend) {
            .log => self.log_observer.observer(),
            .noop => self.noop.observer(),
            .verbose => self.verbose_observer.observer(),
        };
    }
};

pub fn parseObserverBackend(raw: []const u8) ?ObserverBackend {
    if (std.ascii.eqlIgnoreCase(raw, "log")) return .log;
    if (std.ascii.eqlIgnoreCase(raw, "noop")) return .noop;
    if (std.ascii.eqlIgnoreCase(raw, "verbose")) return .verbose;
    return null;
}

fn configuredObserverBackend(alloc: std.mem.Allocator) ObserverBackend {
    const raw = std.process.getEnvVarOwned(alloc, "NULLCLAW_OBSERVER") catch return .log;
    defer alloc.free(raw);
    return parseObserverBackend(raw) orelse .log;
}

fn buildConfig(alloc: std.mem.Allocator, workspace_path: []const u8) !Config {
    var cfg = try Config.load(alloc);
    cfg.workspace_dir = workspace_path;
    return cfg;
}

const RestrictedToolOptions = struct {
    include_shell: bool = false,
    include_memory_list: bool = false,
};

fn appendTool(alloc: std.mem.Allocator, list: *std.ArrayList(tools_mod.Tool), tool_value: anytype) !void {
    const ToolType = @TypeOf(tool_value);
    const ptr = try alloc.create(ToolType);
    ptr.* = tool_value;
    try list.append(alloc, ptr.tool());
}

fn buildRestrictedTools(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    cfg: *const Config,
    opts: RestrictedToolOptions,
) ![]tools_mod.Tool {
    var list: std.ArrayList(tools_mod.Tool) = .{};
    errdefer {
        for (list.items) |t| t.deinit(alloc);
        list.deinit(alloc);
    }

    if (opts.include_shell) {
        try appendTool(alloc, &list, tools_mod.shell.ShellTool{
            .workspace_dir = workspace_path,
            .allowed_paths = cfg.autonomy.allowed_paths,
            .timeout_ns = cfg.tools.shell_timeout_secs * std.time.ns_per_s,
            .max_output_bytes = cfg.tools.shell_max_output_bytes,
        });
    }

    try appendTool(alloc, &list, tools_mod.file_read.FileReadTool{
        .workspace_dir = workspace_path,
        .allowed_paths = cfg.autonomy.allowed_paths,
        .max_file_size = cfg.tools.max_file_size_bytes,
    });

    try appendTool(alloc, &list, tools_mod.memory_recall.MemoryRecallTool{});

    if (opts.include_memory_list) {
        try appendTool(alloc, &list, tools_mod.memory_list.MemoryListTool{});
    }

    return list.toOwnedSlice(alloc);
}

fn buildEchoTools(alloc: std.mem.Allocator, workspace_path: []const u8, cfg: *const Config) ![]tools_mod.Tool {
    return buildRestrictedTools(alloc, workspace_path, cfg, .{
        .include_shell = false,
        .include_memory_list = true,
    });
}

fn buildWardenTools(alloc: std.mem.Allocator, workspace_path: []const u8, cfg: *const Config) ![]tools_mod.Tool {
    return buildRestrictedTools(alloc, workspace_path, cfg, .{
        .include_shell = true,
        .include_memory_list = false,
    });
}

pub fn runEcho(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    system_prompt: []const u8,
    spec_content: []const u8,
    memory_context: []const u8,
) !AgentResult {
    log.info("echo start workspace={s}", .{workspace_path});
    const start = std.time.milliTimestamp();

    var cfg = try buildConfig(alloc, workspace_path);
    defer cfg.deinit();

    var runtime_provider = try providers.runtime_bundle.RuntimeProviderBundle.init(alloc, &cfg);
    defer runtime_provider.deinit();
    const provider_i = runtime_provider.provider();

    const tools = try buildEchoTools(alloc, workspace_path, &cfg);
    defer tools_mod.deinitTools(alloc, tools);

    var mem_rt = memory_mod.initRuntime(alloc, &cfg.memory, workspace_path);
    defer if (mem_rt) |*rt| rt.deinit();
    const mem_opt: ?memory_mod.Memory = if (mem_rt) |rt| rt.memory else null;
    tools_mod.bindMemoryTools(tools, mem_opt);

    var obs_runtime = ObserverRuntime.init(alloc);
    const obs = obs_runtime.observer();

    var agent = try Agent.fromConfig(alloc, &cfg, provider_i, tools, mem_opt, obs);
    defer agent.deinit();

    const message = try std.fmt.allocPrint(alloc,
        \\{s}
        \\
        \\---
        \\## Spec content
        \\
        \\{s}
        \\
        \\{s}
        \\
        \\Produce a plan.json file in the workspace at docs/runs/<run_id>/plan.json.
    , .{ system_prompt, spec_content, memory_context });
    defer alloc.free(message);

    const response = try agent.runSingle(message);
    const owned = try alloc.dupe(u8, response);

    const elapsed_ms = std.time.milliTimestamp() - start;
    log.info("echo done tokens={d} ms={d}", .{ agent.tokensUsed(), elapsed_ms });

    return .{
        .content = owned,
        .token_count = agent.tokensUsed(),
        .wall_seconds = @as(u64, @intCast(@max(0, elapsed_ms))) / 1000,
        .exit_ok = true,
    };
}

pub fn runScout(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    system_prompt: []const u8,
    plan_content: []const u8,
    defects_content: ?[]const u8,
) !AgentResult {
    log.info("scout start workspace={s}", .{workspace_path});
    const start = std.time.milliTimestamp();

    var cfg = try buildConfig(alloc, workspace_path);
    defer cfg.deinit();

    var runtime_provider = try providers.runtime_bundle.RuntimeProviderBundle.init(alloc, &cfg);
    defer runtime_provider.deinit();
    const provider_i = runtime_provider.provider();

    const tools = try tools_mod.allTools(alloc, workspace_path, .{
        .allowed_paths = &.{workspace_path},
        .tools_config = cfg.tools,
    });
    defer tools_mod.deinitTools(alloc, tools);

    var mem_rt = memory_mod.initRuntime(alloc, &cfg.memory, workspace_path);
    defer if (mem_rt) |*rt| rt.deinit();
    const mem_opt: ?memory_mod.Memory = if (mem_rt) |rt| rt.memory else null;
    tools_mod.bindMemoryTools(tools, mem_opt);

    var obs_runtime = ObserverRuntime.init(alloc);
    const obs = obs_runtime.observer();

    var agent = try Agent.fromConfig(alloc, &cfg, provider_i, tools, mem_opt, obs);
    defer agent.deinit();

    const defects_section = if (defects_content) |d|
        try std.fmt.allocPrint(alloc, "\n\n## Defects from previous attempt\n\n{s}", .{d})
    else
        try alloc.dupe(u8, "");
    defer alloc.free(defects_section);

    const message = try std.fmt.allocPrint(alloc,
        \\{s}
        \\
        \\---
        \\## Plan
        \\
        \\{s}
        \\{s}
        \\
        \\Implement the plan. Write code. Produce implementation.md summarising what was done.
    , .{ system_prompt, plan_content, defects_section });
    defer alloc.free(message);

    const response = try agent.runSingle(message);
    const owned = try alloc.dupe(u8, response);

    const elapsed_ms = std.time.milliTimestamp() - start;
    log.info("scout done tokens={d} ms={d}", .{ agent.tokensUsed(), elapsed_ms });

    return .{
        .content = owned,
        .token_count = agent.tokensUsed(),
        .wall_seconds = @as(u64, @intCast(@max(0, elapsed_ms))) / 1000,
        .exit_ok = true,
    };
}

pub fn runWarden(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    system_prompt: []const u8,
    spec_content: []const u8,
    plan_content: []const u8,
    implementation_summary: []const u8,
) !AgentResult {
    log.info("warden start workspace={s}", .{workspace_path});
    const start = std.time.milliTimestamp();

    var cfg = try buildConfig(alloc, workspace_path);
    defer cfg.deinit();

    var runtime_provider = try providers.runtime_bundle.RuntimeProviderBundle.init(alloc, &cfg);
    defer runtime_provider.deinit();
    const provider_i = runtime_provider.provider();

    const tools = try buildWardenTools(alloc, workspace_path, &cfg);
    defer tools_mod.deinitTools(alloc, tools);

    var mem_rt = memory_mod.initRuntime(alloc, &cfg.memory, workspace_path);
    defer if (mem_rt) |*rt| rt.deinit();
    const mem_opt: ?memory_mod.Memory = if (mem_rt) |rt| rt.memory else null;
    tools_mod.bindMemoryTools(tools, mem_opt);

    var obs_runtime = ObserverRuntime.init(alloc);
    const obs = obs_runtime.observer();

    var agent = try Agent.fromConfig(alloc, &cfg, provider_i, tools, mem_opt, obs);
    defer agent.deinit();

    const message = try std.fmt.allocPrint(alloc,
        \\{s}
        \\
        \\---
        \\## Original spec
        \\
        \\{s}
        \\
        \\## Plan that was implemented
        \\
        \\{s}
        \\
        \\## Implementation summary
        \\
        \\{s}
        \\
        \\Review the implementation against the spec. Run tests if present.
        \\Produce validation.md with tiered findings (T1=critical, T2=significant,
        \\T3=minor, T4=suggestion). A PASS verdict requires no T1 or T2 findings.
        \\Extract workspace observations (patterns, pitfalls, learnings) as bullet points
        \\under a "## Workspace observations" section for cross-run memory.
    , .{ system_prompt, spec_content, plan_content, implementation_summary });
    defer alloc.free(message);

    const response = try agent.runSingle(message);
    const owned = try alloc.dupe(u8, response);

    const elapsed_ms = std.time.milliTimestamp() - start;
    log.info("warden done tokens={d} ms={d}", .{ agent.tokensUsed(), elapsed_ms });

    return .{
        .content = owned,
        .token_count = agent.tokensUsed(),
        .wall_seconds = @as(u64, @intCast(@max(0, elapsed_ms))) / 1000,
        .exit_ok = true,
    };
}
