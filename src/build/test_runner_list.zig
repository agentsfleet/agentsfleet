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

/// Emitted once, echoing the directory this binary's root module is rooted at.
/// The checker joins it with each test's namespace to recover a repo-relative path.
const ROOT_PREFIX = "ROOT\t";
/// Emitted once per registered test, carrying the compiler's fully-qualified name
/// (`<namespace>.test.<description>`, namespace being the source path relative to
/// ROOT with `/` replaced by `.` and `.zig` stripped).
const TEST_PREFIX = "TEST\t";
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

    const stdout = Io.File.stdout();
    emit(stdout, ROOT_PREFIX, if (args.len > 1) args[1] else "");
    for (builtin.test_functions) |test_fn| emit(stdout, TEST_PREFIX, test_fn.name);
}

/// Best-effort write: a closed stdout means the checker went away, and a listing
/// run has nothing to clean up, so there is no error worth propagating.
fn emit(file: Io.File, prefix: []const u8, value: []const u8) void {
    file.writeStreamingAll(io, prefix) catch return;
    file.writeStreamingAll(io, value) catch return;
    file.writeStreamingAll(io, LINE_END) catch return;
}
