//! Tool interface — vtable contract for skill-backed tools.
//!
//! Every tool that a Zombie can use (Slack, GitHub, AgentMail, etc.) must
//! implement this interface. The executor bridge calls executeFn with a
//! pre-injected credential from the vault — the tool never fetches credentials
//! itself.
//!
//! To add a new skill tool:
//!   1. Create src/zombie/tools/<name>_tool.zig implementing this interface.
//!   2. Add an entry to src/zombie/tool_registry.zig (name + domains).
//!   Zero other changes required.
//!
//! Separation from NullClaw tools:
//!   NullClaw tools (file_read, shell, memory) are executor-side built-ins that
//!   run inside the agent process. Skill tools defined here are main-server-side
//!   and require vault-injected credentials. They are two independent layers.

const std = @import("std");
const Allocator = std.mem.Allocator;

// Error code constants — canonical source: src/errors/codes.zig.
// Duplicated here so src/zombie/tools/ stays self-contained.
pub const ERR_CREDENTIAL_NOT_FOUND = "UZ-TOOL-001";
pub const ERR_API_FAILED = "UZ-TOOL-002";
pub const ERR_TOOL_NOT_ATTACHED = "UZ-TOOL-004";
pub const ERR_TOOL_UNKNOWN = "UZ-TOOL-005";
pub const ERR_TOOL_TIMEOUT = "UZ-TOOL-006";

/// Result returned by a skill tool's executeFn.
///
/// output is returned to the NullClaw agent after the bridge strips
/// any credential echoes. error_code follows the UZ-TOOL-xxx scheme.
pub const ToolResult = struct {
    success: bool,
    /// Agent-visible output. Bridge strips credential values before delivery.
    output: []const u8,
    /// True when the bridge detected and stripped a credential echo.
    credential_echo: bool = false,
    /// UZ-TOOL-xxx code on failure; null on success.
    error_code: ?[]const u8 = null,
    /// Operator-readable failure description; null on success.
    error_message: ?[]const u8 = null,

    pub fn ok(output: []const u8) ToolResult {
        return .{ .success = true, .output = output };
    }

    pub fn fail(code: []const u8, message: []const u8) ToolResult {
        return .{ .success = false, .output = "", .error_code = code, .error_message = message };
    }
};

/// Error set for tool execution failures.
pub const ToolError = error{
    CredentialNotFound,
    ToolNotAttached,
    ApiCallFailed,
    Timeout,
    InvalidParams,
    OutOfMemory,
};

/// Vtable interface every skill-backed tool must implement.
///
/// Tools are instantiated once and reused across invocations within a session.
/// The executeFn receives a pre-injected credential from the vault —
/// the tool must NOT fetch credentials itself.
///
/// Concrete tool types use ToolVTable(T) to generate a vtable at comptime.
pub const Tool = struct {
    ptr: *anyopaque,
    vtable: *const VTable,

    pub const VTable = struct {
        /// Tool identifier — matches skill name in TRIGGER.md.
        name: []const u8,
        /// One-line description surfaced to the NullClaw agent tool spec.
        description: []const u8,
        /// JSON Schema string for the tool's input parameters.
        parameters_schema: []const u8,
        /// External domains this tool needs (matched against network allowlist).
        required_domains: []const []const u8,

        /// Execute one tool action. credential is pre-injected by the bridge.
        executeFn: *const fn (
            ptr: *anyopaque,
            alloc: Allocator,
            action: []const u8,
            params: std.json.Value,
            credential: []const u8,
        ) ToolError!ToolResult,

        /// Release resources held by this tool instance.
        deinitFn: *const fn (ptr: *anyopaque) void,
    };

    pub fn name(self: Tool) []const u8 {
        return self.vtable.name;
    }

    pub fn execute(
        self: Tool,
        alloc: Allocator,
        action: []const u8,
        params: std.json.Value,
        credential: []const u8,
    ) ToolError!ToolResult {
        return self.vtable.executeFn(self.ptr, alloc, action, params, credential);
    }

    pub fn deinit(self: Tool) void {
        self.vtable.deinitFn(self.ptr);
    }
};

/// Generate a VTable for concrete type T at comptime.
///
/// T must declare:
///   pub const tool_name: []const u8
///   pub const tool_description: []const u8
///   pub const tool_params_schema: []const u8
///   pub const required_domains: []const []const u8
///   pub fn execute(self: *T, alloc, action, params, credential) ToolError!ToolResult
///   pub fn deinit(self: *T) void
pub fn ToolVTable(comptime T: type) Tool.VTable {
    return .{
        .name = T.tool_name,
        .description = T.tool_description,
        .parameters_schema = T.tool_params_schema,
        .required_domains = T.required_domains,
        .executeFn = struct {
            fn execute(
                ptr: *anyopaque,
                alloc: Allocator,
                action: []const u8,
                params: std.json.Value,
                credential: []const u8,
            ) ToolError!ToolResult {
                return @as(*T, @ptrCast(@alignCast(ptr))).execute(alloc, action, params, credential);
            }
        }.execute,
        .deinitFn = struct {
            fn deinit(ptr: *anyopaque) void {
                @as(*T, @ptrCast(@alignCast(ptr))).deinit();
            }
        }.deinit,
    };
}

// ── Tests ──────────────────────────────────────────────────────────────────

test "ToolResult.ok sets success=true with output" {
    const r = ToolResult.ok("hello world");
    try std.testing.expect(r.success);
    try std.testing.expectEqualStrings("hello world", r.output);
    try std.testing.expect(r.error_code == null);
    try std.testing.expect(r.error_message == null);
    try std.testing.expect(!r.credential_echo);
}

test "ToolResult.fail sets success=false with code and message" {
    const r = ToolResult.fail(ERR_API_FAILED, "API returned 403 Forbidden");
    try std.testing.expect(!r.success);
    try std.testing.expectEqualStrings("", r.output);
    try std.testing.expectEqualStrings(ERR_API_FAILED, r.error_code.?);
    try std.testing.expectEqualStrings("API returned 403 Forbidden", r.error_message.?);
}

test "ToolVTable dispatches through vtable correctly" {
    // Minimal concrete tool for vtable dispatch verification.
    const EchoTool = struct {
        const Self = @This();
        pub const tool_name = "echo";
        pub const tool_description = "Returns params as output (test only)";
        pub const tool_params_schema = "{}";
        pub const required_domains: []const []const u8 = &.{};

        pub fn execute(
            _: *Self,
            _: Allocator,
            action: []const u8,
            _: std.json.Value,
            _: []const u8,
        ) ToolError!ToolResult {
            return ToolResult.ok(action);
        }

        pub fn deinit(_: *Self) void {}

        const vtable = ToolVTable(@This());
        pub fn tool(self: *Self) Tool {
            return .{ .ptr = @ptrCast(self), .vtable = &vtable };
        }
    };

    var echo = EchoTool{};
    const t = echo.tool();

    try std.testing.expectEqualStrings("echo", t.name());
    const result = try t.execute(std.testing.allocator, "ping", .null, "");
    try std.testing.expect(result.success);
    try std.testing.expectEqualStrings("ping", result.output);
}
