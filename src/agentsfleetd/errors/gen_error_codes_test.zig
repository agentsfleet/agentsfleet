// Regenerating error-codes.mdx from the registry is a pure function of
// REGISTRY; running it twice must produce byte-identical output
// ("re-running on a clean tree is a no-op").
const std = @import("std");
const build_options = @import("build_options");
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

test "gen_error_codes.render() follows the documentation reference shape" {
    const alloc = std.testing.allocator;
    var run: std.Io.Writer.Allocating = .init(alloc);
    defer run.deinit();
    try gen.render(alloc, &run.writer);
    const out = run.written();

    const ordered = [_][]const u8{
        "type: reference\n",
        "audience: user\n",
        "verified: 2026-07-12\n",
        "product_version: " ++ build_options.version ++ "\n",
        "executable: false\n",
        "# Error codes\n",
        "## Synopsis\n",
        "## Example with output\n",
        "## Options\n",
        "## Errors\n",
        "## Related pages\n",
    };
    var offset: usize = 0;
    for (ordered) |needle| {
        const relative = std.mem.indexOf(u8, out[offset..], needle) orelse {
            std.debug.print("generated page is missing ordered section: {s}\n", .{needle});
            return error.TestExpectedSectionInOutput;
        };
        offset += relative + needle.len;
    }
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

        var anchor_buf: [96]u8 = undefined;
        const anchor = try std.fmt.bufPrint(&anchor_buf, "<span id=\"{s}\"></span>", .{entry.code});
        try std.testing.expect(std.mem.indexOf(u8, out, anchor) != null);
    }
}

test "gen_error_codes.render() excludes removed command spellings" {
    const alloc = std.testing.allocator;
    var run: std.Io.Writer.Allocating = .init(alloc);
    defer run.deinit();
    try gen.render(alloc, &run.writer);
    const out = run.written();

    const removed_spellings = [_][]const u8{
        "agentsfleet install --from",
        "agentsfleet secret add",
        "agentsfleet workspace add",
        "agentsfleet tenant provider add",
    };
    for (removed_spellings) |spelling| {
        try std.testing.expect(std.mem.indexOf(u8, out, spelling) == null);
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

    try std.testing.expect(std.mem.indexOf(u8, out, "\n### Webhooks\n") != null);
    try std.testing.expect(std.mem.indexOf(u8, out, "\n### Fleets\n") != null);
    try std.testing.expect(std.mem.indexOf(u8, out, "\n### Tenant models\n") != null);
}

test "gen_error_codes.render() gives webhook failures webhook-specific prevention" {
    const alloc = std.testing.allocator;
    var run: std.Io.Writer.Allocating = .init(alloc);
    defer run.deinit();
    try gen.render(alloc, &run.writer);
    const out = run.written();

    try std.testing.expect(std.mem.indexOf(u8, out, "Keep webhook signing secrets matched and service clocks synchronized.") != null);
    try std.testing.expect(std.mem.indexOf(u8, out, "Refresh credentials before they expire.") == null);
}

test "gen_error_codes.render() documents required and optional response fields" {
    const alloc = std.testing.allocator;
    var run: std.Io.Writer.Allocating = .init(alloc);
    defer run.deinit();
    try gen.render(alloc, &run.writer);
    const out = run.written();

    try std.testing.expect(std.mem.indexOf(u8, out, "Every response contains the first five fields below.") != null);
    try std.testing.expect(std.mem.indexOf(u8, out, "| `current_state` |") != null);
    try std.testing.expect(std.mem.indexOf(u8, out, "| `user_message` |") != null);
}

test "gen_error_codes.render() prefers public user messages" {
    const alloc = std.testing.allocator;
    var run: std.Io.Writer.Allocating = .init(alloc);
    defer run.deinit();
    try gen.render(alloc, &run.writer);
    const out = run.written();

    try std.testing.expect(std.mem.indexOf(u8, out, "We couldn't finish that request.") != null);
    try std.testing.expect(std.mem.indexOf(u8, out, "Check the err= field and database logs.") == null);
    try std.testing.expect(std.mem.indexOf(u8, out, "This page comes from the `agentsfleetd` error registry.") == null);
}

test "gen_error_codes.render() keeps operator details out of public fixes" {
    const alloc = std.testing.allocator;
    var run: std.Io.Writer.Allocating = .init(alloc);
    defer run.deinit();
    try gen.render(alloc, &run.writer);
    const out = run.written();

    const private_terms = [_][]const u8{
        "err= field",
        "DATABASE_URL",
        "REDIS_URL_API",
        "Postgres memory schema",
        "core.tenant_model_entries",
        "core.model_library",
        "core.platform_provider_defaults",
        "API_MAX_",
        "SSE_MAX_",
        "config_json",
    };
    for (private_terms) |term| {
        try std.testing.expect(std.mem.indexOf(u8, out, term) == null);
    }
}
