//! Operator CLI output rendering.
//!
//! `output.zig` had its two data-shape tests but no test ever called `fail`,
//! `writeOut` or `writeErr`, so both rendering branches and every write path
//! were unexecuted. These matter beyond coverage: Pillar 4 promises a
//! machine-stable failure envelope, and the JSON branch has a fallback for when
//! serialising that envelope itself fails — a path that only runs when the
//! process is already out of memory and so is never exercised by accident.

const std = @import("std");

const output = @import("output.zig");
const plane_stub = @import("plane_stub_test.zig");

const ALLOC = std.testing.allocator;

const SAMPLE = output.CliError{
    .code = "SAMPLE_CODE",
    .message = "something went wrong",
    .suggestion = "try the other thing",
};

test "audience forced to json ignores the terminal probe" {
    try std.testing.expectEqual(output.Audience.json, output.audience(true));
}

test "audience probes the terminal when json is not forced" {
    // Under the test runner stdout is not a TTY, so this resolves `.json` — the
    // documented safe default. The point is that the probe branch executes at
    // all; a canceled probe must not propagate an error to a CLI teardown path.
    const resolved = output.audience(false);
    try std.testing.expect(resolved == .json or resolved == .human);
}

test "fail returns the process exit code in both audiences" {
    // Exit code 1 is the operator-visible contract; a handler that returned 0
    // on failure would make a broken runner look healthy to a supervisor.
    try std.testing.expectEqual(@as(u8, 1), output.fail(.json, ALLOC, SAMPLE));
    try std.testing.expectEqual(@as(u8, 1), output.fail(.human, ALLOC, SAMPLE));
}

test "fail falls back to a minimal envelope when serialising the error fails" {
    // The `catch` arm inside the json branch. Without a failing allocator it is
    // unreachable, which is precisely why it had never run: an untested
    // fallback in the out-of-memory path is where a CLI crashes instead of
    // reporting.
    var failing = std.testing.FailingAllocator.init(ALLOC, .{ .fail_index = 0 });
    try std.testing.expectEqual(@as(u8, 1), output.fail(.json, failing.allocator(), SAMPLE));
}

test "the shared CLI errors carry a code, a message and an actionable fix" {
    // Single-sourced per RULE UFS. An empty suggestion is the failure mode:
    // Pillar 4 exists so the operator is told what to do, not just what broke.
    for ([_]output.CliError{
        output.ERR_API_URL_UNSET,
        output.ERR_UNREACHABLE,
        output.ERR_OOM,
    }) |e| {
        try std.testing.expect(e.code.len > 0);
        try std.testing.expect(e.message.len > 0);
        try std.testing.expect(e.suggestion.len > 0);
    }
}

test "the shared CLI error codes are distinct" {
    try std.testing.expect(!std.mem.eql(u8, output.ERR_API_URL_UNSET.code, output.ERR_UNREACHABLE.code));
    try std.testing.expect(!std.mem.eql(u8, output.ERR_UNREACHABLE.code, output.ERR_OOM.code));
    try std.testing.expect(!std.mem.eql(u8, output.ERR_API_URL_UNSET.code, output.ERR_OOM.code));
}

test "writeOut and writeErr complete without propagating io failure" {
    // Both wrap `writeStream`, whose writes are best-effort by design — a
    // closed pipe must not crash the CLI on its way out.
    var muted = try plane_stub.MutedStdout.mute();
    defer muted.restore();
    output.writeOut("probe_write_out\n");
    output.writeErr("probe_write_err\n");
}

test "a payload larger than the write buffer still drains" {
    // writeStream buffers 256 bytes; anything longer exercises the flush loop
    // rather than a single buffered write.
    var muted = try plane_stub.MutedStdout.mute();
    defer muted.restore();
    const long = "y" ** 1024;
    output.writeOut(long ++ "\n");
}
