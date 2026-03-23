const std = @import("std");
const builtin = @import("builtin");

pub const ValidationError = error{
    InvalidSandboxBackend,
    InvalidSandboxKillGraceMs,
};

pub const PreflightError = error{
    BubblewrapUnavailable,
    UnsupportedPlatform,
};

pub const Backend = enum {
    host,
    bubblewrap,

    pub fn label(self: Backend) []const u8 {
        return @tagName(self);
    }
};

pub const Config = struct {
    backend: Backend = defaultBackend(),
    kill_grace_ms: u64 = 250,

    pub fn label(self: Config) []const u8 {
        return self.backend.label();
    }

    pub fn preflight(self: Config) PreflightError!void {
        switch (self.backend) {
            .host => return,
            .bubblewrap => {
                if (builtin.os.tag != .linux) return PreflightError.UnsupportedPlatform;
                if (!isExecutableOnPath("bwrap")) return PreflightError.BubblewrapUnavailable;
            },
        }
    }
};

pub const ToolExecutionContext = struct {
    cancel_flag: ?*const std.atomic.Value(bool) = null,
    deadline_ms: ?i64 = null,
    sandbox: Config = .{},
    run_id: []const u8 = "",
    workspace_id: []const u8 = "",
    request_id: []const u8 = "",
    trace_id: []const u8 = "",
    stage_id: []const u8 = "",
    role_id: []const u8 = "",
    skill_id: []const u8 = "",
};

pub fn parseBackend(raw: []const u8) ?Backend {
    if (std.ascii.eqlIgnoreCase(raw, "auto")) return defaultBackend();
    if (std.ascii.eqlIgnoreCase(raw, "host")) return .host;
    if (std.ascii.eqlIgnoreCase(raw, "bubblewrap")) return .bubblewrap;
    return null;
}

pub fn defaultBackend() Backend {
    return if (builtin.os.tag == .linux) .bubblewrap else .host;
}

pub fn loadFromEnv(alloc: std.mem.Allocator) (ValidationError || std.mem.Allocator.Error)!Config {
    const backend = try loadBackendFromEnv(alloc, "SANDBOX_BACKEND");
    const kill_grace_ms = try parseU64Env(
        alloc,
        "SANDBOX_KILL_GRACE_MS",
        250,
        ValidationError.InvalidSandboxKillGraceMs,
    );
    if (kill_grace_ms == 0) return ValidationError.InvalidSandboxKillGraceMs;

    return .{
        .backend = backend,
        .kill_grace_ms = kill_grace_ms,
    };
}

fn loadBackendFromEnv(
    alloc: std.mem.Allocator,
    name: []const u8,
) (ValidationError || std.mem.Allocator.Error)!Backend {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return defaultBackend();
    defer alloc.free(raw);
    return parseBackend(std.mem.trim(u8, raw, " \t\r\n")) orelse ValidationError.InvalidSandboxBackend;
}

fn parseU64Env(
    alloc: std.mem.Allocator,
    name: []const u8,
    default_value: u64,
    invalid_error: ValidationError,
) (ValidationError || std.mem.Allocator.Error)!u64 {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return default_value;
    defer alloc.free(raw);
    return std.fmt.parseInt(u64, std.mem.trim(u8, raw, " \t\r\n"), 10) catch invalid_error;
}

fn isExecutableOnPath(name: []const u8) bool {
    const path_value = std.process.getEnvVarOwned(std.heap.page_allocator, "PATH") catch return false;
    defer std.heap.page_allocator.free(path_value);

    var path_iter = std.mem.tokenizeScalar(u8, path_value, std.fs.path.delimiter);
    while (path_iter.next()) |dir_path| {
        if (dir_path.len == 0) continue;
        const full_path = std.fs.path.join(std.heap.page_allocator, &.{ dir_path, name }) catch continue;
        defer std.heap.page_allocator.free(full_path);
        std.fs.accessAbsolute(full_path, .{ .mode = .read_only }) catch continue;
        return true;
    }
    return false;
}

test "defaultBackend is host on non-linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    try std.testing.expectEqual(Backend.host, defaultBackend());
}

test "parseBackend supports auto and explicit values" {
    try std.testing.expectEqual(Backend.host, parseBackend("host").?);
    try std.testing.expectEqual(defaultBackend(), parseBackend("auto").?);
    try std.testing.expectEqual(Backend.bubblewrap, parseBackend("bubblewrap").?);
    try std.testing.expect(parseBackend("bogus") == null);
}

test "bubblewrap preflight fails on non-linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    const cfg = Config{
        .backend = .bubblewrap,
    };
    try std.testing.expectError(PreflightError.UnsupportedPlatform, cfg.preflight());
}
