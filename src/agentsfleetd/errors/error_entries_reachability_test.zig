// A dashboard-reachable code always needs a curated `user_message` (authored
// via eu(), not e()). The standing invariant: every `e()` entry carries a
// `// reachable: no — <reason>` annotation, on the LINE CONTAINING THE ENTRY'S
// CLOSING PAREN (not necessarily the opening `e(` line — a multi-line hint
// built with `++` puts the annotation on its last continuation line); an
// entry marked `// reachable: yes` while still using `e()` (not `eu()`) is a
// contradiction — that entry should have been promoted to eu() instead — and
// fails the audit. The mudball-detail and mudball-justification guards live
// in internal_op_error_sweep_test.zig; this file covers reachability.
const std = @import("std");
const common = @import("common");
const sweep = @import("internal_op_error_sweep_test.zig");

const REGISTRY_FILES = [_][]const u8{
    "src/agentsfleetd/errors/error_entries.zig",
    "src/agentsfleetd/errors/error_entries_runtime.zig",
};

// Matches "    e(" but not "    eu(" — "eu(" never contains the substring
// "e(" (the char after 'e' is 'u', not '('), so no eu() call ever matches.
const E_CALL_NEEDLE = "    e(";
const REACHABLE_YES_MARKER = "reachable: yes";
const REACHABLE_NO_MARKER = "reachable: no";

/// Fails if any bare `e(` (not `eu(`) entry is marked `reachable: yes`
/// (contradiction — should be eu()) or carries no `reachable:` annotation at
/// all (the authoring rule requires one on every e()-only entry).
pub fn scanForReachableWithoutUserMessage(path: []const u8, content: []const u8) !void {
    var idx: usize = 0;
    while (std.mem.indexOfPos(u8, content, idx, E_CALL_NEEDLE)) |pos| {
        const open_paren_pos = pos + E_CALL_NEEDLE.len - 1;
        const close_paren_pos = sweep.matchingCloseParen(content, open_paren_pos) orelse content.len;
        idx = close_paren_pos + 1;

        const line_start = if (std.mem.lastIndexOfScalar(u8, content[0..close_paren_pos], '\n')) |i| i + 1 else 0;
        const line_end = std.mem.indexOfScalarPos(u8, content, close_paren_pos, '\n') orelse content.len;
        const line = content[line_start..line_end];

        if (std.mem.indexOf(u8, line, REACHABLE_YES_MARKER) != null) {
            std.debug.print("e() entry marked reachable:yes without a user_message (not eu()) in {s}: \"{s}\"\n", .{ path, line });
            return error.TestUnexpectedResult;
        }
        if (std.mem.indexOf(u8, line, REACHABLE_NO_MARKER) == null) {
            std.debug.print("e() entry missing a `// reachable: no — <reason>` annotation in {s}: \"{s}\"\n", .{ path, line });
            return error.TestUnexpectedResult;
        }
    }
}

test "no e()-only registry entry is marked reachable:yes, and every one carries a reachable annotation" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();

    for (REGISTRY_FILES) |path| {
        const content = try std.Io.Dir.cwd().readFileAlloc(io, path, alloc, .limited(256 * 1024));
        defer alloc.free(content);
        try scanForReachableWithoutUserMessage(path, content);
    }
}

test "guard: a fixture e() entry marked reachable:yes is caught" {
    const fixture =
        \\    // audit-error-codes: intentional-fake
        \\    e("UZ-FIX-001", .bad_request, "Fixture", "detail text"), // reachable: yes — should have been eu()
    ;
    try std.testing.expectError(error.TestUnexpectedResult, scanForReachableWithoutUserMessage("fixture.zig", fixture));
}

test "guard: a fixture e() entry with no reachable annotation at all is caught" {
    const fixture =
        \\    // audit-error-codes: intentional-fake
        \\    e("UZ-FIX-001", .bad_request, "Fixture", "detail text"),
    ;
    try std.testing.expectError(error.TestUnexpectedResult, scanForReachableWithoutUserMessage("fixture.zig", fixture));
}

test "guard: a fixture e() entry marked reachable:no passes" {
    const fixture =
        \\    // audit-error-codes: intentional-fake
        \\    e("UZ-FIX-001", .bad_request, "Fixture", "detail text"), // reachable: no — CLI-only surface
    ;
    try scanForReachableWithoutUserMessage("fixture.zig", fixture);
}

test "guard: a multi-line e() entry's annotation on its closing line is found (not just the opening line)" {
    const fixture =
        \\    // audit-error-codes: intentional-fake
        \\    e("UZ-FIX-001", .bad_request, "Fixture", "first part of a long hint " ++
        \\        "second part of the hint"), // reachable: no — CLI-only surface
    ;
    try scanForReachableWithoutUserMessage("fixture.zig", fixture);
}

test "guard: a multi-line e() entry marked reachable:yes on its closing line is caught" {
    const fixture =
        \\    // audit-error-codes: intentional-fake
        \\    e("UZ-FIX-001", .bad_request, "Fixture", "first part of a long hint " ++
        \\        "second part of the hint"), // reachable: yes — should have been eu()
    ;
    try std.testing.expectError(error.TestUnexpectedResult, scanForReachableWithoutUserMessage("fixture.zig", fixture));
}

// The authoring rule (distinct failure => distinct code; reachable =>
// user_message; error-codes.mdx is generated) is documented at
// error_entries.zig's own module doc comment — the project-local home, since
// docs/LOGGING_STANDARD.md and audits/error-codes.sh both resolve to dotfiles
// symlinks.
test "the authoring rule is documented in error_entries.zig" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();
    const content = try std.Io.Dir.cwd().readFileAlloc(io, "src/agentsfleetd/errors/error_entries.zig", alloc, .limited(256 * 1024));
    defer alloc.free(content);

    try std.testing.expect(std.mem.indexOf(u8, content, "Distinct failure") != null);
    try std.testing.expect(std.mem.indexOf(u8, content, "Reachable => user_message") != null);
    try std.testing.expect(std.mem.indexOf(u8, content, "error-codes.mdx is generated") != null);
}
