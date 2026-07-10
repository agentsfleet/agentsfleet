//! Prints the tests a binary actually registered, instead of running them.
//!
//! Zig registers a file's `test` blocks only when the file is force-referenced at
//! comptime from a test root (`test { _ = @import("x.zig"); }`). An ordinary
//! `const x = @import("x.zig")` registers nothing, even when the importing file is
//! itself analyzed — so a `test` block can sit on disk for months, be counted by any
//! textual scan of `src/`, and never compile. The compiler is therefore the only
//! sound authority on which blocks are live, and `builtin.test_functions` is where
//! it records them.
//!
//! Installed ONLY on the `list-tests` lane (`src/build/test_list.zig`), never on the
//! real `test` steps: those keep Zig's default runner, whose `std.zig.Server` stdio
//! protocol is what `zig build test --summary all` reports pass/skip totals from.
//! Consumer of this output: `scripts/check_zig_test_reachability.py`.

const std = @import("std");
const builtin = @import("builtin");
const Io = std.Io;

/// Emitted once, proving this lane ran even when it registers no tests at all.
const ROOT_PREFIX = "ROOT\t";
/// Emitted once per registered test as `TEST\t<root_dir>\t<name>`, where `<name>` is
/// the compiler's fully-qualified `<namespace>.test.<description>` and `<namespace>`
/// is the source path relative to `<root_dir>` with `/` replaced by `.` and `.zig`
/// stripped. Each line carries its own root because `zig build` runs the lanes on a
/// thread pool: attributing a test to the most recent `ROOT` line would misfile it
/// the day two lanes interleave on stdout, silently flipping a file's live/dead
/// verdict — the exact failure this gate exists to catch.
const TEST_PREFIX = "TEST\t";
const FIELD_SEP = "\t";
const LINE_END = "\n";

/// `std.process.Init.Minimal.args.toSlice` needs an allocator; argv here is one
/// short path, so a fixed buffer avoids pulling a general-purpose allocator into
/// the test binary.
const ARGV_SCRATCH_BYTES = 8 * 1024;
var argv_scratch: [ARGV_SCRATCH_BYTES]u8 = undefined;

const io: Io = Io.Threaded.global_single_threaded.io();

pub fn main(init: std.process.Init.Minimal) void {
    var fba = std.heap.FixedBufferAllocator.init(&argv_scratch);
    const args = init.args.toSlice(fba.allocator()) catch
        @panic("test_runner_list: cannot read argv");

    const root_dir = if (args.len > 1) args[1] else "";
    const stdout = Io.File.stdout();
    emit(stdout, &.{ ROOT_PREFIX, root_dir });
    for (builtin.test_functions) |test_fn| {
        emit(stdout, &.{ TEST_PREFIX, root_dir, FIELD_SEP, test_fn.name });
    }
}

/// Best-effort write: a closed stdout means the checker went away, and a listing
/// run has nothing to clean up, so there is no error worth propagating.
fn emit(file: Io.File, parts: []const []const u8) void {
    for (parts) |part| file.writeStreamingAll(io, part) catch return;
    file.writeStreamingAll(io, LINE_END) catch return;
}
