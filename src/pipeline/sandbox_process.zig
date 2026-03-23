const std = @import("std");
const builtin = @import("builtin");

const AtomicBool = std.atomic.Value(bool);

pub const RunResult = struct {
    stdout: []u8,
    stderr: []u8,
    success: bool,
    exit_code: ?u32 = null,
    interrupted: bool = false,

    pub fn deinit(self: *const RunResult, allocator: std.mem.Allocator) void {
        if (self.stdout.len > 0) allocator.free(self.stdout);
        if (self.stderr.len > 0) allocator.free(self.stderr);
    }
};

pub const RunOptions = struct {
    cwd: ?[]const u8 = null,
    env_map: ?*std.process.EnvMap = null,
    max_output_bytes: usize = 1_048_576,
    cancel_flag: ?*const AtomicBool = null,
    kill_grace_ms: u64 = 250,
};

const CancelWatcherCtx = struct {
    child: *std.process.Child,
    cancel_flag: *const AtomicBool,
    done: *AtomicBool,
    kill_grace_ms: u64,
};

fn cancelWatcherMain(ctx: *CancelWatcherCtx) void {
    while (!ctx.done.load(.acquire)) {
        if (ctx.cancel_flag.load(.acquire)) {
            terminateChildTree(ctx.child, ctx.kill_grace_ms);
            break;
        }
        std.Thread.sleep(20 * std.time.ns_per_ms);
    }
}

fn terminateChildTree(child: *std.process.Child, kill_grace_ms: u64) void {
    if (builtin.os.tag == .windows) {
        std.os.windows.TerminateProcess(child.id, 1) catch {};
        return;
    }

    const group_id: std.posix.pid_t = -child.id;
    std.posix.kill(group_id, std.posix.SIG.TERM) catch {};
    std.Thread.sleep(kill_grace_ms * std.time.ns_per_ms);
    std.posix.kill(group_id, std.posix.SIG.KILL) catch {};
}

fn appendUtf8Replacement(out: *std.ArrayListUnmanaged(u8), allocator: std.mem.Allocator) !void {
    try out.appendSlice(allocator, "\xEF\xBF\xBD");
}

fn normalizeUtf8Lossy(allocator: std.mem.Allocator, input: []u8) ![]u8 {
    if (std.unicode.utf8ValidateSlice(input)) return input;

    var out: std.ArrayListUnmanaged(u8) = .empty;
    errdefer out.deinit(allocator);
    try out.ensureTotalCapacity(allocator, input.len);

    var index: usize = 0;
    while (index < input.len) {
        const first = input[index];
        if (first < 0x80) {
            try out.append(allocator, first);
            index += 1;
            continue;
        }

        const seq_len = std.unicode.utf8ByteSequenceLength(first) catch {
            try appendUtf8Replacement(&out, allocator);
            index += 1;
            continue;
        };
        const step: usize = @intCast(seq_len);
        if (index + step > input.len) {
            try appendUtf8Replacement(&out, allocator);
            index += 1;
            continue;
        }

        const candidate = input[index .. index + step];
        _ = std.unicode.utf8Decode(candidate) catch {
            try appendUtf8Replacement(&out, allocator);
            index += 1;
            continue;
        };
        try out.appendSlice(allocator, candidate);
        index += step;
    }

    allocator.free(input);
    return out.toOwnedSlice(allocator);
}

pub fn run(
    allocator: std.mem.Allocator,
    argv: []const []const u8,
    opts: RunOptions,
) !RunResult {
    var child = std.process.Child.init(argv, allocator);
    child.stdout_behavior = .Pipe;
    child.stderr_behavior = .Pipe;
    if (builtin.os.tag != .windows and builtin.os.tag != .wasi) {
        child.pgid = 0;
    }
    if (opts.cwd) |cwd| child.cwd = cwd;
    if (opts.env_map) |env| child.env_map = env;

    try child.spawn();

    var cancel_done = AtomicBool.init(false);
    var cancel_watcher: ?std.Thread = null;
    var watcher_ctx: CancelWatcherCtx = undefined;
    if (opts.cancel_flag) |flag| {
        watcher_ctx = .{
            .child = &child,
            .cancel_flag = flag,
            .done = &cancel_done,
            .kill_grace_ms = opts.kill_grace_ms,
        };
        cancel_watcher = std.Thread.spawn(.{}, cancelWatcherMain, .{&watcher_ctx}) catch null;
    }
    defer {
        cancel_done.store(true, .release);
        if (cancel_watcher) |thread| thread.join();
    }

    var stdout = if (child.stdout) |stdout_file| blk: {
        break :blk stdout_file.readToEndAlloc(allocator, opts.max_output_bytes) catch |err| {
            if (opts.cancel_flag != null and opts.cancel_flag.?.load(.acquire)) {
                break :blk try allocator.dupe(u8, "");
            }
            return err;
        };
    } else try allocator.dupe(u8, "");
    errdefer allocator.free(stdout);
    stdout = try normalizeUtf8Lossy(allocator, stdout);

    var stderr = if (child.stderr) |stderr_file| blk: {
        break :blk stderr_file.readToEndAlloc(allocator, opts.max_output_bytes) catch |err| {
            if (opts.cancel_flag != null and opts.cancel_flag.?.load(.acquire)) {
                break :blk try allocator.dupe(u8, "");
            }
            return err;
        };
    } else try allocator.dupe(u8, "");
    errdefer allocator.free(stderr);
    stderr = try normalizeUtf8Lossy(allocator, stderr);

    const term = try child.wait();
    const interrupted = if (opts.cancel_flag) |flag| flag.load(.acquire) else false;

    return switch (term) {
        .Exited => |code| .{
            .stdout = stdout,
            .stderr = stderr,
            .success = code == 0,
            .exit_code = code,
            .interrupted = interrupted,
        },
        else => .{
            .stdout = stdout,
            .stderr = stderr,
            .success = false,
            .exit_code = null,
            .interrupted = interrupted,
        },
    };
}

test "run returns stdout" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;
    const result = try run(std.testing.allocator, &.{ "echo", "hello" }, .{});
    defer result.deinit(std.testing.allocator);

    try std.testing.expect(result.success);
    try std.testing.expect(std.mem.indexOf(u8, result.stdout, "hello") != null);
}

test "run kill switch terminates command tree" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    var cancel = AtomicBool.init(false);
    const ResultBox = struct {
        result: ?RunResult = null,
        err: ?anyerror = null,
    };
    var box = ResultBox{};

    const Runner = struct {
        fn entry(out: *ResultBox, cancel_flag: *AtomicBool) void {
            out.result = run(std.testing.allocator, &.{ "sh", "-c", "sleep 5; echo done" }, .{
                .cancel_flag = cancel_flag,
                .kill_grace_ms = 50,
            }) catch |err| {
                out.err = err;
                return;
            };
        }
    };

    const thread = try std.Thread.spawn(.{}, Runner.entry, .{ &box, &cancel });
    std.Thread.sleep(100 * std.time.ns_per_ms);
    cancel.store(true, .release);
    thread.join();

    try std.testing.expect(box.err == null);
    const result = box.result orelse return error.TestUnexpectedResult;
    defer result.deinit(std.testing.allocator);
    try std.testing.expect(!result.success);
    try std.testing.expect(result.interrupted);
}
