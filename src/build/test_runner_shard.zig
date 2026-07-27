//! Executes a disjoint slice of a test binary's registered tests, so one
//! compiled binary can be driven by several concurrent processes.
//!
//! Zig 0.16's default runner walks `builtin.test_functions` start to finish on
//! one thread (`Io.Threaded.global_single_threaded`). That is the floor under
//! every lane that runs a test binary: the daemon unit binary alone registers
//! thousands of tests and is executed three times per change — plain, under
//! kcov, and under Valgrind's 10-30x slowdown. Sharding turns those into N
//! processes over the same artifact.
//!
//! **This file replaces the runner that detects `std.testing.allocator` leaks.**
//! Every behaviour the upstream terminal runner has on that path is reproduced
//! here deliberately, not incidentally: the per-test allocator reset, the `Io`
//! instance lifecycle, the leak tally, the logged-error tally, and the exit
//! condition that fails on any of the three. A divergence is a silently
//! weakened gate, which is worse than a slow one — it reports green. The
//! equivalence is asserted by `scripts/check_zig_shard_runner_test.py`, which
//! runs a deliberately leaking binary and requires a non-zero exit, and
//! compares sharded verdicts against the unsharded set.
//!
//! Selection is by index modulo count over the compiler-registered order. With
//! no shard environment set, one shard owns every test, so a lane that has not
//! been migrated behaves exactly as it did before.

const builtin = @import("builtin");
const std = @import("std");
const Io = std.Io;
const testing = std.testing;
const native_os = builtin.os.tag;

/// Read by this runner, written by `scripts/run-zig-shards.sh`. The two spell
/// the same identifiers verbatim; a rename that touches only one side silently
/// collapses every shard onto the whole suite, which reads as a slow pass
/// rather than as a fault.
pub const SHARD_INDEX_ENV = "AGENTSFLEET_TEST_SHARD_INDEX";
pub const SHARD_COUNT_ENV = "AGENTSFLEET_TEST_SHARD_COUNT";

/// Absent or empty shard environment means "one shard owns everything".
const SINGLE_SHARD: Shard = .{ .index = 0, .count = 1 };

/// Exit code for a malformed shard environment. Distinct from the test-failure
/// code so a harness bug is never mistaken for a red suite.
const EXIT_BAD_SHARD_ENV = 2;
const EXIT_TEST_FAILURE = 1;

pub const std_options: std.Options = .{
    .logFn = log,
};

var log_err_count: usize = 0;

/// Diagnostics go to stderr through `Io.File` rather than `std.debug.print`,
/// matching `test_runner_list.zig`. `std.log` is not an option here: this file
/// *defines* the log function, so routing its own output through it would
/// recurse and, worse, inflate `log_err_count` — the very counter the exit
/// condition reads.
const io: Io = Io.Threaded.global_single_threaded.io();

/// Formatting scratch. A registered test name is a namespaced path plus a
/// description; 4 KiB clears the longest in this tree with room to spare, and
/// an overflow degrades to a marker rather than losing the verdict line.
var emit_buffer: [4096]u8 = undefined;

/// Best-effort, exactly like the sibling runner: a closed stderr means nobody
/// is reading, and the process exit code still carries the verdict.
fn emit(comptime fmt: []const u8, args: anytype) void {
    @disableInstrumentation();
    const rendered = std.fmt.bufPrint(&emit_buffer, fmt, args) catch "<line too long to render>\n";
    Io.File.stderr().writeStreamingAll(io, rendered) catch return;
}

const Shard = struct {
    index: usize,
    count: usize,

    /// A test belongs to this shard when its registered position selects it.
    /// Modulo rather than contiguous blocks: registration order groups tests by
    /// source file, and whole files differ in cost by orders of magnitude, so
    /// contiguous blocks would hand one shard every slow suite.
    fn owns(self: Shard, position: usize) bool {
        return position % self.count == self.index;
    }
};

const Tally = struct {
    ok: usize = 0,
    skipped: usize = 0,
    failed: usize = 0,
    leaked: usize = 0,

    fn clean(self: Tally) bool {
        return self.failed == 0 and self.leaked == 0 and log_err_count == 0;
    }
};

pub fn main(init: std.process.Init.Minimal) void {
    @disableInstrumentation();

    const shard = parseShard(init.environ) orelse std.process.exit(EXIT_BAD_SHARD_ENV);

    var tally: Tally = .{};
    for (builtin.test_functions, 0..) |test_fn, position| {
        if (!shard.owns(position)) continue;
        runOne(test_fn, position, init.args, init.environ, &tally);
    }
    report(shard, tally);

    if (!tally.clean()) std.process.exit(EXIT_TEST_FAILURE);
}

/// One test, with the exact per-test lifecycle the upstream terminal runner
/// uses. The allocator and `Io` instance are rebuilt per test so a leak is
/// attributed to the test that caused it rather than to whichever ran last.
fn runOne(
    test_fn: std.builtin.TestFn,
    position: usize,
    args: std.process.Args,
    environ: std.process.Environ,
    tally: *Tally,
) void {
    @disableInstrumentation();

    testing.allocator_instance = .{};
    testing.io_instance = .init(testing.allocator, .{
        .argv0 = .init(args),
        .environ = environ,
    });
    defer {
        testing.io_instance.deinit();
        if (testing.allocator_instance.deinit() == .leak) tally.leaked += 1;
    }
    testing.log_level = .warn;
    testing.environ = environ;

    emit("{d} {s}...", .{ position, test_fn.name });
    if (test_fn.func()) |_| {
        tally.ok += 1;
        emit("OK\n", .{});
    } else |err| switch (err) {
        error.SkipZigTest => {
            tally.skipped += 1;
            emit("SKIP\n", .{});
        },
        else => {
            tally.failed += 1;
            emit("FAIL ({t})\n", .{err});
            if (@errorReturnTrace()) |trace| std.debug.dumpErrorReturnTrace(trace);
        },
    }
}

/// Machine-readable so the fan-out can aggregate without parsing prose, and
/// human-readable so a single-shard run still reads like the default runner.
fn report(shard: Shard, tally: Tally) void {
    @disableInstrumentation();
    emit(
        "shard {d}/{d}: {d} passed; {d} skipped; {d} failed; {d} leaked; {d} errors logged.\n",
        .{ shard.index, shard.count, tally.ok, tally.skipped, tally.failed, tally.leaked, log_err_count },
    );
}

/// `null` means the environment is malformed and the caller must abort. An
/// unreadable shard selector is never treated as "run everything": that would
/// turn a typo into N copies of the full suite, each passing, and the lane
/// would look merely slow while proving nothing about the partition.
fn parseShard(environ: std.process.Environ) ?Shard {
    @disableInstrumentation();

    const raw_count = envValue(environ, SHARD_COUNT_ENV) orelse return SINGLE_SHARD;
    const raw_index = envValue(environ, SHARD_INDEX_ENV) orelse return SINGLE_SHARD;
    if (raw_count.len == 0 or raw_index.len == 0) return SINGLE_SHARD;

    const count = std.fmt.parseInt(usize, raw_count, 10) catch
        return reject(SHARD_COUNT_ENV, raw_count);
    const index = std.fmt.parseInt(usize, raw_index, 10) catch
        return reject(SHARD_INDEX_ENV, raw_index);

    if (count == 0) return reject(SHARD_COUNT_ENV, raw_count);
    if (index >= count) return reject(SHARD_INDEX_ENV, raw_index);
    return .{ .index = index, .count = count };
}

fn reject(name: []const u8, value: []const u8) ?Shard {
    @disableInstrumentation();
    emit("test_runner_shard: invalid {s}={s}\n", .{ name, value });
    return null;
}

/// The shard selector is a POSIX-only convenience; the lanes that set it run on
/// Linux and macOS. Anywhere else falls back to a single shard rather than
/// failing, because "no sharding" is always a correct way to run every test.
fn envValue(environ: std.process.Environ, key: []const u8) ?[]const u8 {
    @disableInstrumentation();
    if (native_os == .windows) return null;
    return environ.getPosix(key);
}

pub fn log(
    comptime message_level: std.log.Level,
    comptime scope: @EnumLiteral(),
    comptime format: []const u8,
    args: anytype,
) void {
    @disableInstrumentation();
    if (@intFromEnum(message_level) <= @intFromEnum(std.log.Level.err)) {
        log_err_count +|= 1;
    }
    if (@intFromEnum(message_level) <= @intFromEnum(testing.log_level)) {
        emit(
            "[" ++ @tagName(scope) ++ "] (" ++ @tagName(message_level) ++ "): " ++ format ++ "\n",
            args,
        );
    }
}
