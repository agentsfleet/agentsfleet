// Regenerating error-codes.mdx from the registry is a pure function of
// REGISTRY; running it twice must produce byte-identical output
// ("re-running on a clean tree is a no-op").
const std = @import("std");
const gen = @import("gen_error_codes.zig");

test "gen_error_codes.render() is idempotent" {
    const alloc = std.testing.allocator;

    var run1: std.Io.Writer.Allocating = .init(alloc);
    defer run1.deinit();
    try gen.render(alloc, &run1.writer);

    var run2: std.Io.Writer.Allocating = .init(alloc);
    defer run2.deinit();
    try gen.render(alloc, &run2.writer);

    try std.testing.expectEqualStrings(run1.written(), run2.written());
}

test "gen_error_codes.render() output contains every REGISTRY code exactly once" {
    const alloc = std.testing.allocator;
    var run: std.Io.Writer.Allocating = .init(alloc);
    defer run.deinit();
    try gen.render(alloc, &run.writer);
    const out = run.written();

    const entries = @import("error_entries.zig");
    const entries_runtime = @import("error_entries_runtime.zig");
    const registry = entries.ENTRIES ++ entries_runtime.ENTRIES_RUNTIME;

    for (registry) |entry| {
        var needle_buf: [64]u8 = undefined;
        const needle = try std.fmt.bufPrint(&needle_buf, "`{s}`", .{entry.code});
        const first = std.mem.indexOf(u8, out, needle) orelse {
            std.debug.print("code missing from generated mdx: {s}\n", .{entry.code});
            return error.TestExpectedCodeInOutput;
        };
        const second = std.mem.indexOf(u8, out[first + needle.len ..], needle);
        try std.testing.expect(second == null);
    }
}
