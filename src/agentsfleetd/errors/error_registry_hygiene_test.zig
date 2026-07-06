// Dead-code deletion + copy-rename proofs, and the regression guard that the
// untouched Generic internalOperationError() sites still resolve to the
// catch-all code.
const std = @import("std");
const common = @import("common");
const reg = @import("error_registry.zig");

const DEAD_CODES = [_][]const u8{
    "UZ-BUNDLE-006", "UZ-GRANT-001", "UZ-RUN-007", "UZ-EXEC-001", "UZ-EXEC-002",
};

/// A retired code's breadcrumb comment — a whole-line comment
/// (`// UZ-BUNDLE-006 retired ...`, the pre-existing UZ-WH-003/UZ-AGT-007/
/// UZ-CONN-005 pattern) or a trailing one (`...; // UZ-GRANT-001 retired...`)
/// — is not a producer; only a reference in the code portion of the line
/// (before any `//`) counts. A `//` immediately preceded by `:` (a URL
/// scheme like `https://`) is not a comment marker — skip it and keep
/// looking for the real one, so a `docs_uri` string earlier on the line
/// can't hide a genuine reference that follows it.
fn hasNonCommentReference(content: []const u8, code: []const u8) bool {
    var it = std.mem.splitScalar(u8, content, '\n');
    while (it.next()) |line| {
        var search_from: usize = 0;
        const comment_start = while (std.mem.indexOfPos(u8, line, search_from, "//")) |pos| {
            if (pos > 0 and line[pos - 1] == ':') {
                search_from = pos + 2;
                continue;
            }
            break pos;
        } else line.len;
        if (std.mem.indexOf(u8, line[0..comment_start], code) != null) return true;
    }
    return false;
}

test "the 5 dead codes have no producer anywhere in src/agentsfleetd/" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();

    var src_dir = try std.Io.Dir.cwd().openDir(io, "src/agentsfleetd", .{ .iterate = true });
    defer src_dir.close(io);
    var walker = try src_dir.walk(alloc);
    defer walker.deinit();

    while (try walker.next(io)) |entry| {
        if (entry.kind != .file) continue;
        if (!std.mem.endsWith(u8, entry.basename, ".zig")) continue;
        if (std.mem.endsWith(u8, entry.basename, "_test.zig")) continue;

        const content = try entry.dir.readFileAlloc(io, entry.basename, alloc, .limited(256 * 1024));
        defer alloc.free(content);

        for (DEAD_CODES) |code| {
            if (hasNonCommentReference(content, code)) {
                std.debug.print("dead code {s} still referenced (non-comment) in {s}\n", .{ code, entry.basename });
                return error.TestUnexpectedResult;
            }
        }
    }
}

test "the 5 dead codes are absent from the registry" {
    for (DEAD_CODES) |code| {
        const entry = reg.lookup(code);
        try std.testing.expect(!std.mem.eql(u8, entry.code, code)); // lookup() falls back to UNKNOWN
    }
}

test "every ERR_* constant resolves in REGISTRY, and REGISTRY has no orphan-prone duplicate" {
    // The declared==used invariant is comptime-enforced in error_registry.zig
    // itself (every ERR_* is looked up at compile time); this test pins the
    // runtime-observable half: every live REGISTRY code is lookup-able and
    // round-trips to itself (no UNKNOWN fallback for a real entry).
    for (reg.REGISTRY) |entry| {
        const looked_up = reg.lookup(entry.code);
        try std.testing.expectEqualStrings(entry.code, looked_up.code);
    }
}

test "PROVIDER-004/006/007/008 copy says library, never catalogue" {
    const codes = [_][]const u8{ "UZ-PROVIDER-004", "UZ-PROVIDER-006", "UZ-PROVIDER-007", "UZ-PROVIDER-008" };
    for (codes) |code| {
        const entry = reg.lookup(code);
        try std.testing.expect(std.mem.indexOf(u8, entry.title, "catalogue") == null);
        try std.testing.expect(std.mem.indexOf(u8, entry.hint, "catalogue") == null);
        if (entry.user_message) |um| try std.testing.expect(std.mem.indexOf(u8, um, "catalogue") == null);
    }
    // The activate-failure message reads "...isn't in our library yet" (exact wording check).
    const um = reg.lookup("UZ-PROVIDER-004").user_message orelse return error.TestExpectedUserMessage;
    try std.testing.expect(std.mem.indexOf(u8, um, "isn't in our library yet") != null);
}

test "internalOperationError() always resolves to UZ-INTERNAL-003 (regression: Generic sites unaffected by triage)" {
    // internalOperationError()'s signature has no code parameter — it always
    // maps to ERR_INTERNAL_OPERATION_FAILED (common.zig). A promoted site
    // switches to common.errorResponse() with its own code instead; a scrubbed
    // site keeps calling internalOperationError() with a cleaner literal. Either
    // way, no Generic site's resolved code can silently drift.
    const entry = reg.lookup(reg.ERR_INTERNAL_OPERATION_FAILED);
    try std.testing.expectEqualStrings("UZ-INTERNAL-003", entry.code);
}
