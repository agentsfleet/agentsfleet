const std = @import("std");
const builtin = @import("builtin");
const nullclaw = @import("nullclaw");

const platform = nullclaw.platform;
const root = nullclaw.tools;
const JsonObjectMap = root.JsonObjectMap;
const Tool = root.Tool;
const ToolResult = root.ToolResult;
const isResolvedPathAllowed = nullclaw.tools.path_security.isResolvedPathAllowed;
const SecurityPolicy = nullclaw.security.SecurityPolicy;

const metrics = @import("../observability/metrics.zig");
const error_codes = @import("../errors/codes.zig");
const sandbox_process = @import("sandbox_process.zig");
const sandbox_runtime = @import("sandbox_runtime.zig");

const DEFAULT_SHELL_TIMEOUT_NS: u64 = 60 * std.time.ns_per_s;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 1_048_576;
const UNAVAILABLE_WORKSPACE_SENTINEL = "/__usezombie_workspace_unavailable__";
const log = std.log.scoped(.sandbox_shell);

const SAFE_ENV_VARS: []const []const u8 = if (builtin.os.tag == .windows)
    &.{ "PATH", "HOME", "TERM", "LANG", "LC_ALL", "LC_CTYPE", "USER", "SHELL", "TMPDIR", "TEMP", "TMP" }
else
    &.{ "PATH", "HOME", "TERM", "LANG", "LC_ALL", "LC_CTYPE", "USER", "SHELL", "TMPDIR" };

const CancelReason = enum(u8) {
    none = 0,
    timeout = 1,
    parent_cancel = 2,
};

const ShellToolError = error{
    InvalidCwd,
    CwdOutsideAllowedAreas,
    WorkspaceUnavailable,
    SandboxBackendUnavailable,
};

pub const SandboxShellTool = struct {
    workspace_dir: []const u8,
    allowed_paths: []const []const u8 = &.{},
    timeout_ns: u64 = DEFAULT_SHELL_TIMEOUT_NS,
    max_output_bytes: usize = DEFAULT_MAX_OUTPUT_BYTES,
    cancel_flag: ?*const std.atomic.Value(bool) = null,
    deadline_ms: ?i64 = null,
    sandbox: sandbox_runtime.Config = .{},
    run_id: []const u8 = "",
    workspace_id: []const u8 = "",
    request_id: []const u8 = "",
    trace_id: []const u8 = "",
    stage_id: []const u8 = "",
    role_id: []const u8 = "",
    skill_id: []const u8 = "",
    policy: ?*const SecurityPolicy = null,

    pub const tool_name = "shell";
    pub const tool_description = "Execute a shell command in the workspace directory";
    pub const tool_params =
        \\{"type":"object","properties":{"command":{"type":"string","description":"The shell command to execute"},"cwd":{"type":"string","description":"Working directory (absolute path within allowed paths; defaults to workspace)"}},"required":["command"]}
    ;

    const vtable = root.ToolVTable(@This());

    pub fn tool(self: *SandboxShellTool) Tool {
        return .{ .ptr = @ptrCast(self), .vtable = &vtable };
    }

    pub fn execute(self: *SandboxShellTool, allocator: std.mem.Allocator, args: JsonObjectMap) !ToolResult {
        const command_input = root.getString(args, "command") orelse return ToolResult.fail("Missing 'command' parameter");
        const command = normalizeCommandInput(command_input);

        if (self.policy) |policy| {
            _ = policy.validateCommandExecution(command, false) catch |err| {
                return switch (err) {
                    error.CommandNotAllowed => blk: {
                        log.warn("sandbox.command_blocked error_code={s} backend={s} run_id={s} workspace_id={s} request_id={s} trace_id={s} stage_id={s} role_id={s} skill_id={s} command={s} bytes={d}", .{
                            error_codes.ERR_SANDBOX_COMMAND_BLOCKED,
                            self.sandbox.label(),
                            self.run_id,
                            self.workspace_id,
                            self.request_id,
                            self.trace_id,
                            self.stage_id,
                            self.role_id,
                            self.skill_id,
                            command,
                            command.len,
                        });
                        break :blk ToolResult.fail("Command not allowed by security policy");
                    },
                    error.HighRiskBlocked => ToolResult.fail("High-risk command blocked by security policy"),
                    error.ApprovalRequired => ToolResult.fail("Command requires approval"),
                };
            };
        }

        const effective_cwd = self.resolveCwd(allocator, args) catch |err| {
            return switch (err) {
                ShellToolError.InvalidCwd => ToolResult.fail("cwd must be an absolute path"),
                ShellToolError.CwdOutsideAllowedAreas => ToolResult.fail("cwd is outside allowed areas"),
                ShellToolError.WorkspaceUnavailable => ToolResult.fail("cwd not allowed"),
                else => return err,
            };
        };
        self.sandbox.preflight() catch |err| {
            metrics.incSandboxPreflightFailures();
            log.err("sandbox.preflight_fail error_code={s} backend={s} run_id={s} workspace_id={s} request_id={s} trace_id={s} stage_id={s} role_id={s} skill_id={s} err={s}", .{
                error_codes.ERR_SANDBOX_BACKEND_UNAVAILABLE,
                self.sandbox.label(),
                self.run_id,
                self.workspace_id,
                self.request_id,
                self.trace_id,
                self.stage_id,
                self.role_id,
                self.skill_id,
                @errorName(err),
            });
            return ToolResult{ .success = false, .output = "", .error_msg = try std.fmt.allocPrint(
                allocator,
                "{s}: sandbox backend {s} is not available",
                .{ error_codes.ERR_SANDBOX_BACKEND_UNAVAILABLE, self.sandbox.label() },
            ) };
        };
        var env = try self.buildEnv(allocator);
        defer env.deinit();

        var deadline_cancel = std.atomic.Value(bool).init(false);
        var cancel_reason = std.atomic.Value(u8).init(@intFromEnum(CancelReason.none));
        var watcher_done = std.atomic.Value(bool).init(false);
        var watcher: ?std.Thread = null;

        if (self.cancel_flag != null or self.deadline_ms != null) {
            watcher = std.Thread.spawn(.{}, cancelWatcherMain, .{ CancelWatcherCtx{
                .parent_cancel = self.cancel_flag,
                .deadline_ms = self.deadline_ms,
                .deadline_cancel = &deadline_cancel,
                .cancel_reason = &cancel_reason,
                .done = &watcher_done,
            } }) catch null;
        }
        defer {
            watcher_done.store(true, .release);
            if (watcher) |thread| thread.join();
        }

        const timeout_ns = effectiveTimeoutNs(self.timeout_ns, self.deadline_ms);
        if (timeout_ns == 0) {
            return self.killSwitchFailure(allocator, .timeout);
        }

        var timeout_thread: ?std.Thread = null;
        if (timeout_ns > 0) {
            timeout_thread = std.Thread.spawn(.{}, timeoutMain, .{ TimeoutCtx{
                .cancel_flag = &deadline_cancel,
                .cancel_reason = &cancel_reason,
                .done = &watcher_done,
                .timeout_ns = timeout_ns,
            } }) catch null;
        }
        defer if (timeout_thread) |thread| thread.join();

        const run_result = try self.runCommand(allocator, effective_cwd, &env, command, &deadline_cancel);
        defer allocator.free(run_result.stderr);

        metrics.incSandboxShellRuns();
        switch (self.sandbox.backend) {
            .host => metrics.incSandboxHostRuns(),
            .bubblewrap => metrics.incSandboxBubblewrapRuns(),
        }

        if (run_result.success) {
            if (run_result.stdout.len > 0) return ToolResult{ .success = true, .output = run_result.stdout };
            allocator.free(run_result.stdout);
            return ToolResult{ .success = true, .output = try allocator.dupe(u8, "(no output)") };
        }
        defer allocator.free(run_result.stdout);

        if (run_result.interrupted) {
            return self.killSwitchFailure(
                allocator,
                @enumFromInt(cancel_reason.load(.acquire)),
            );
        }
        if (run_result.exit_code != null) {
            const message = if (run_result.stderr.len > 0) run_result.stderr else "Command failed with non-zero exit code";
            return ToolResult{ .success = false, .output = "", .error_msg = try allocator.dupe(u8, message) };
        }
        return ToolResult{ .success = false, .output = "", .error_msg = "Command terminated by signal" };
    }

    fn resolveCwd(self: *SandboxShellTool, allocator: std.mem.Allocator, args: JsonObjectMap) ShellToolError![]const u8 {
        if (root.getString(args, "cwd")) |cwd| {
            if (cwd.len == 0 or !std.fs.path.isAbsolute(cwd)) return ShellToolError.InvalidCwd;

            const resolved_cwd = std.fs.cwd().realpathAlloc(allocator, cwd) catch {
                return ShellToolError.InvalidCwd;
            };
            defer allocator.free(resolved_cwd);

            const ws_resolved = std.fs.cwd().realpathAlloc(allocator, self.workspace_dir) catch null;
            defer if (ws_resolved) |value| allocator.free(value);
            if (ws_resolved == null and self.allowed_paths.len == 0) {
                return ShellToolError.WorkspaceUnavailable;
            }
            if (!isResolvedPathAllowed(
                allocator,
                resolved_cwd,
                ws_resolved orelse UNAVAILABLE_WORKSPACE_SENTINEL,
                self.allowed_paths,
            )) {
                return ShellToolError.CwdOutsideAllowedAreas;
            }
            return cwd;
        }
        return self.workspace_dir;
    }

    fn buildEnv(self: *SandboxShellTool, allocator: std.mem.Allocator) !std.process.EnvMap {
        _ = self;
        var env = std.process.EnvMap.init(allocator);
        for (SAFE_ENV_VARS) |key| {
            if (platform.getEnvOrNull(allocator, key)) |value| {
                defer allocator.free(value);
                try env.put(key, value);
            }
        }
        return env;
    }

    fn runCommand(
        self: *SandboxShellTool,
        allocator: std.mem.Allocator,
        effective_cwd: []const u8,
        env: *std.process.EnvMap,
        command: []const u8,
        cancel_flag: *const std.atomic.Value(bool),
    ) !sandbox_process.RunResult {
        const argv = try self.buildArgv(allocator, effective_cwd, command);
        defer allocator.free(argv);
        return sandbox_process.run(allocator, argv, .{
            .cwd = effective_cwd,
            .env_map = env,
            .max_output_bytes = self.max_output_bytes,
            .cancel_flag = cancel_flag,
            .kill_grace_ms = self.sandbox.kill_grace_ms,
        });
    }

    fn buildArgv(
        self: *SandboxShellTool,
        allocator: std.mem.Allocator,
        effective_cwd: []const u8,
        command: []const u8,
    ) ![]const []const u8 {
        var argv: std.ArrayList([]const u8) = .{};
        errdefer argv.deinit(allocator);

        if (self.sandbox.backend == .bubblewrap) {
            try argv.appendSlice(allocator, &.{ "bwrap", "--die-with-parent", "--unshare-all", "--dev", "/dev", "--proc", "/proc", "--tmpfs", "/tmp" });
            try appendBindIfExists(allocator, &argv, "/usr", true);
            try appendBindIfExists(allocator, &argv, "/bin", true);
            try appendBindIfExists(allocator, &argv, "/sbin", true);
            try appendBindIfExists(allocator, &argv, "/lib", true);
            try appendBindIfExists(allocator, &argv, "/lib64", true);
            try appendBindIfExists(allocator, &argv, "/etc", true);
            _ = effective_cwd;
            try argv.appendSlice(allocator, &.{ "--bind", self.workspace_dir, "/workspace", "--chdir", "/workspace" });
        }

        try argv.append(allocator, platform.getShell());
        try argv.append(allocator, platform.getShellFlag());
        try argv.append(allocator, command);
        return argv.toOwnedSlice(allocator);
    }

    fn killSwitchFailure(
        self: *SandboxShellTool,
        allocator: std.mem.Allocator,
        reason: CancelReason,
    ) !ToolResult {
        metrics.incSandboxKillSwitches();
        log.warn("sandbox.kill_switch error_code={s} backend={s} run_id={s} workspace_id={s} request_id={s} trace_id={s} stage_id={s} role_id={s} skill_id={s} reason={s}", .{
            error_codes.ERR_SANDBOX_KILL_SWITCH_TRIGGERED,
            self.sandbox.label(),
            self.run_id,
            self.workspace_id,
            self.request_id,
            self.trace_id,
            self.stage_id,
            self.role_id,
            self.skill_id,
            @tagName(reason),
        });
        const message = switch (reason) {
            .timeout => try std.fmt.allocPrint(
                allocator,
                "{s}: command timed out and the process tree was terminated",
                .{error_codes.ERR_SANDBOX_KILL_SWITCH_TRIGGERED},
            ),
            .parent_cancel => try std.fmt.allocPrint(
                allocator,
                "{s}: worker shutdown/deadline triggered sandbox teardown",
                .{error_codes.ERR_SANDBOX_KILL_SWITCH_TRIGGERED},
            ),
            .none => try std.fmt.allocPrint(
                allocator,
                "{s}: sandbox interrupted command execution",
                .{error_codes.ERR_SANDBOX_KILL_SWITCH_TRIGGERED},
            ),
        };
        return ToolResult{ .success = false, .output = "", .error_msg = message };
    }
};

const CancelWatcherCtx = struct {
    parent_cancel: ?*const std.atomic.Value(bool),
    deadline_ms: ?i64,
    deadline_cancel: *std.atomic.Value(bool),
    cancel_reason: *std.atomic.Value(u8),
    done: *std.atomic.Value(bool),
};

fn cancelWatcherMain(ctx: CancelWatcherCtx) void {
    while (!ctx.done.load(.acquire)) {
        if (ctx.parent_cancel) |flag| {
            if (!flag.load(.acquire)) {
                ctx.cancel_reason.store(@intFromEnum(CancelReason.parent_cancel), .release);
                ctx.deadline_cancel.store(true, .release);
                return;
            }
        }
        if (ctx.deadline_ms) |deadline_ms| {
            if (std.time.milliTimestamp() > deadline_ms) {
                ctx.cancel_reason.store(@intFromEnum(CancelReason.parent_cancel), .release);
                ctx.deadline_cancel.store(true, .release);
                return;
            }
        }
        std.Thread.sleep(20 * std.time.ns_per_ms);
    }
}

const TimeoutCtx = struct {
    cancel_flag: *std.atomic.Value(bool),
    cancel_reason: *std.atomic.Value(u8),
    done: *std.atomic.Value(bool),
    timeout_ns: u64,
};

fn timeoutMain(ctx: TimeoutCtx) void {
    std.Thread.sleep(ctx.timeout_ns);
    if (!ctx.done.load(.acquire)) {
        ctx.cancel_reason.store(@intFromEnum(CancelReason.timeout), .release);
        ctx.cancel_flag.store(true, .release);
    }
}

fn normalizeCommandInput(command: []const u8) []const u8 {
    return std.mem.trim(u8, command, " \t\r\n");
}

fn effectiveTimeoutNs(tool_timeout_ns: u64, deadline_ms: ?i64) u64 {
    if (deadline_ms) |value| {
        const remaining_ms = value - std.time.milliTimestamp();
        if (remaining_ms <= 0) return 0;
        const remaining_ns = @as(u64, @intCast(remaining_ms)) * std.time.ns_per_ms;
        return @min(tool_timeout_ns, remaining_ns);
    }
    return tool_timeout_ns;
}

fn appendBindIfExists(
    allocator: std.mem.Allocator,
    argv: *std.ArrayList([]const u8),
    path: []const u8,
    read_only: bool,
) !void {
    std.fs.accessAbsolute(path, .{}) catch return;
    try argv.append(allocator, if (read_only) "--ro-bind" else "--bind");
    try argv.append(allocator, path);
    try argv.append(allocator, path);
}

test "effectiveTimeoutNs respects nearest deadline" {
    const deadline_ms = std.time.milliTimestamp() + 100;
    try std.testing.expect(effectiveTimeoutNs(5 * std.time.ns_per_s, deadline_ms) <= 100 * std.time.ns_per_ms);
}
