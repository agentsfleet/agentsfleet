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

// Regression guard for the Greptile finding on the docs PR: section headings
// were raw registry namespace tokens ("Wh", "Slk", "Agt", …), opaque to an
// external reader. Every category token currently in REGISTRY must render as
// its friendly CATEGORY_LABELS entry, not the bare capitalized token.
test "gen_error_codes.render() section headings are friendly labels, not raw namespace tokens" {
    const alloc = std.testing.allocator;
    var run: std.Io.Writer.Allocating = .init(alloc);
    defer run.deinit();
    try gen.render(alloc, &run.writer);
    const out = run.written();

    // Excludes STARTUP: its friendly label ("Startup") happens to be
    // identical to the naive capitalized-token fallback — nothing to
    // distinguish there, so it's not part of this regression check.
    const RAW_TOKEN_HEADINGS = [_][]const u8{
        "\n## Wh\n",   "\n## Slk\n",     "\n## Agt\n",      "\n## Gh\n",
        "\n## Conn\n", "\n## Mem\n",     "\n## Req\n",      "\n## Cred\n",
        "\n## Exec\n", "\n## Run\n",     "\n## Uuidv7\n",   "\n## Apikey\n",
        "\n## Vault\n", "\n## Grant\n",  "\n## Bundle\n",   "\n## Provider\n",
        "\n## Tool\n", "\n## Fleetkey\n", "\n## Approval\n",
        "\n## Auth\n",
    };
    for (RAW_TOKEN_HEADINGS) |needle| {
        if (std.mem.indexOf(u8, out, needle) != null) {
            std.debug.print("raw namespace token used as a section heading: \"{s}\"\n", .{needle});
            return error.TestUnexpectedResult;
        }
    }
    try std.testing.expect(std.mem.indexOf(u8, out, "\n## Webhooks\n") != null);
    try std.testing.expect(std.mem.indexOf(u8, out, "\n## Fleets\n") != null);
}
