//! Test runner that assigns compiler-registered tests to deterministic shards.

const builtin = @import("builtin");
const std = @import("std");
const Io = std.Io;
const testing = std.testing;

const SHARD_INDEX_ARG = "--shard-index=";
const SHARD_COUNT_ARG = "--shard-count=";
const LIST_SELECTED_ARG = "--list-selected";
const ISOLATED_TEST_MARKER = "daemon boot -> SIGTERM -> drain";
const FNV_OFFSET_BASIS: u64 = 14_695_981_039_346_656_037;
const FNV_PRIME: u64 = 1_099_511_628_211;
const ARG_SCRATCH_BYTES = 8 * 1024;

var arg_scratch: [ARG_SCRATCH_BYTES]u8 = undefined;
var log_error_count: usize = 0;
const runner_io: Io = Io.Threaded.global_single_threaded.io();

pub const std_options: std.Options = .{ .logFn = log };

const Config = struct {
    index: usize = 0,
    count: usize = 1,
    list_selected: bool = false,
};

pub fn main(init: std.process.Init.Minimal) void {
    @disableInstrumentation();
    var fba = std.heap.FixedBufferAllocator.init(&arg_scratch);
    const args = init.args.toSlice(fba.allocator()) catch
        @panic("test shard runner cannot read arguments");
    const config = parseConfig(args[1..]) catch |err|
        std.debug.panic("invalid test shard configuration: {t}", .{err});

    const selected_count = countSelected(config);
    if (selected_count == 0) {
        emit("shard {d}/{d} selected no tests\n", .{ config.index, config.count });
        std.process.exit(1);
    }
    if (config.list_selected) {
        for (builtin.test_functions) |test_fn| {
            if (belongsToShard(test_fn.name, config)) emit("{s}\n", .{test_fn.name});
        }
        return;
    }

    runSelected(init, config, selected_count);
}

fn parseConfig(args: []const []const u8) !Config {
    var config: Config = .{};
    for (args) |arg| {
        if (std.mem.startsWith(u8, arg, SHARD_INDEX_ARG)) {
            config.index = try std.fmt.parseUnsigned(usize, arg[SHARD_INDEX_ARG.len..], 10);
        } else if (std.mem.startsWith(u8, arg, SHARD_COUNT_ARG)) {
            config.count = try std.fmt.parseUnsigned(usize, arg[SHARD_COUNT_ARG.len..], 10);
        } else if (std.mem.eql(u8, arg, LIST_SELECTED_ARG)) {
            config.list_selected = true;
        } else {
            return error.UnknownArgument;
        }
    }
    if (config.count == 0 or config.index >= config.count) return error.InvalidShard;
    return config;
}

fn countSelected(config: Config) usize {
    var count: usize = 0;
    for (builtin.test_functions) |test_fn| {
        count += @intFromBool(belongsToShard(test_fn.name, config));
    }
    return count;
}

fn belongsToShard(name: []const u8, config: Config) bool {
    if (config.count > 1) {
        const isolated = std.mem.indexOf(u8, name, ISOLATED_TEST_MARKER) != null;
        if (isolated) return config.index == config.count - 1;
        if (config.index == config.count - 1) return false;
    }
    var hash = FNV_OFFSET_BASIS;
    for (name) |byte| {
        hash = (hash ^ byte) *% FNV_PRIME;
    }
    const regular_shards = if (config.count > 1) config.count - 1 else 1;
    return hash % regular_shards == config.index;
}

fn runSelected(init: std.process.Init.Minimal, config: Config, selected_count: usize) void {
    var passed: usize = 0;
    var skipped: usize = 0;
    var failed: usize = 0;
    var leaks: usize = 0;
    var ordinal: usize = 0;
    const have_tty = Io.File.stderr().isTty(runner_io) catch false;

    emit("shard {d}/{d} selected {d} tests\n", .{ config.index, config.count, selected_count });
    for (builtin.test_functions) |test_fn| {
        if (!belongsToShard(test_fn.name, config)) continue;
        ordinal += 1;
        testing.allocator_instance = .{};
        testing.io_instance = .init(testing.allocator, .{
            .argv0 = .init(init.args),
            .environ = init.environ,
        });
        defer {
            testing.io_instance.deinit();
            if (testing.allocator_instance.deinit() == .leak) leaks += 1;
        }
        testing.log_level = .warn;
        testing.environ = init.environ;

        if (!have_tty) emit("{d}/{d} {s}...", .{ ordinal, selected_count, test_fn.name });
        if (test_fn.func()) |_| {
            passed += 1;
            if (!have_tty) emit("OK\n", .{});
        } else |err| switch (err) {
            error.SkipZigTest => {
                skipped += 1;
                if (have_tty) {
                    emit("{d}/{d} {s}...SKIP\n", .{ ordinal, selected_count, test_fn.name });
                } else {
                    emit("SKIP\n", .{});
                }
            },
            else => {
                failed += 1;
                if (have_tty) {
                    emit("{d}/{d} {s}...FAIL ({t})\n", .{ ordinal, selected_count, test_fn.name, err });
                } else {
                    emit("FAIL ({t})\n", .{err});
                }
                if (@errorReturnTrace()) |trace| std.debug.dumpErrorReturnTrace(trace);
            },
        }
    }

    emit("{d} passed; {d} skipped; {d} failed.\n", .{ passed, skipped, failed });
    if (log_error_count != 0) emit("{d} errors were logged.\n", .{log_error_count});
    if (leaks != 0) emit("{d} tests leaked memory.\n", .{leaks});
    if (leaks != 0 or log_error_count != 0 or failed != 0) std.process.exit(1);
}

fn emit(comptime format: []const u8, args: anytype) void {
    var buffer: [4096]u8 = undefined;
    var writer = Io.File.stderr().writerStreaming(runner_io, &buffer);
    writer.interface.print(format, args) catch return;
    writer.interface.flush() catch return;
}

fn log(
    comptime message_level: std.log.Level,
    comptime scope: @EnumLiteral(),
    comptime format: []const u8,
    args: anytype,
) void {
    @disableInstrumentation();
    if (@intFromEnum(message_level) <= @intFromEnum(std.log.Level.err)) log_error_count +|= 1;
    if (@intFromEnum(message_level) <= @intFromEnum(testing.log_level)) {
        emit("[" ++ @tagName(scope) ++ "] (" ++ @tagName(message_level) ++ "): " ++ format ++ "\n", args);
    }
}
