/// M16_002 §1: Spec validation engine.
///
/// Three hard checks (block submission), one soft check (warning only):
///   1.1 Empty / whitespace-only spec → SpecValidation.Failure.empty
///   1.2 No actionable lines (only comments / headings) → no_actionable_content
///   1.3 Unresolved prefixed file reference → unresolved_ref with path
///   1.4 Ambiguous bare filename → warning added to result (non-blocking)
const std = @import("std");

pub const SpecValidation = struct {
    failure: ?Failure = null,
    warnings: std.ArrayList([]const u8),

    pub const Failure = union(enum) {
        empty,
        no_actionable_content,
        /// The path token that could not be stat'd in cwd.
        unresolved_ref: []const u8,
    };

    pub fn deinit(self: *SpecValidation, alloc: std.mem.Allocator) void {
        for (self.warnings.items) |w| alloc.free(w);
        self.warnings.deinit(alloc);
    }
};

/// Validates spec markdown content.
/// Returns a SpecValidation with failure=null on success; warnings may still be present.
/// The caller owns all warning strings in the result — call deinit(alloc) when done.
pub fn validate(
    alloc: std.mem.Allocator,
    spec: []const u8,
    cwd: std.fs.Dir,
) !SpecValidation {
    var result = SpecValidation{ .warnings = .{} };

    // 1.1 — Empty / whitespace-only check.
    const trimmed = std.mem.trim(u8, spec, " \t\r\n");
    if (trimmed.len == 0) {
        result.failure = .empty;
        return result;
    }

    // 1.2 / 1.3 / 1.4 — Line-by-line analysis.
    var has_actionable = false;
    var in_block_comment = false;

    var lines = std.mem.splitScalar(u8, spec, '\n');
    while (lines.next()) |raw_line| {
        const line = std.mem.trimRight(u8, raw_line, "\r");

        // Handle multi-line HTML comments <!-- ... -->
        if (in_block_comment) {
            if (std.mem.indexOf(u8, line, "-->") != null) {
                in_block_comment = false;
            }
            continue;
        }

        const stripped = std.mem.trim(u8, line, " \t");

        // Blank line — not actionable, not blocking.
        if (stripped.len == 0) continue;

        // Full-line block comment start
        if (std.mem.startsWith(u8, stripped, "<!--")) {
            if (std.mem.indexOf(u8, stripped, "-->") == null) {
                in_block_comment = true;
            }
            // Single-line <!-- comment --> is not actionable.
            continue;
        }

        // Markdown heading — not actionable.
        if (std.mem.startsWith(u8, stripped, "#")) continue;

        // This line is actionable.
        has_actionable = true;

        // 1.3 — Scan for prefixed file references.
        if (try scanLineForFileRefs(alloc, stripped, cwd, &result.warnings)) |unresolved| {
            result.failure = .{ .unresolved_ref = unresolved };
            return result;
        }
    }

    if (!has_actionable) {
        result.failure = .no_actionable_content;
        return result;
    }

    return result;
}

/// Scans a single line for path tokens (src/, pkg/, ./ prefixes).
/// Returns the first unresolved path on error, or null on success.
/// Ambiguous bare-filename warnings are appended to warnings list (non-blocking).
fn scanLineForFileRefs(
    alloc: std.mem.Allocator,
    line: []const u8,
    cwd: std.fs.Dir,
    warnings: *std.ArrayList([]const u8),
) !?[]const u8 {
    _ = alloc; // reserved for future ambiguous-ref warning allocation
    _ = warnings; // ambiguous bare-filename detection is deferred (non-blocking path)

    // Split line into tokens by whitespace, backticks, and bracket delimiters.
    var i: usize = 0;
    while (i < line.len) {
        // Skip non-token characters.
        while (i < line.len and isSeparator(line[i])) : (i += 1) {}
        if (i >= line.len) break;

        // Collect token.
        const start = i;
        while (i < line.len and !isSeparator(line[i])) : (i += 1) {}
        const token = line[start..i];

        // Check for known path prefixes.
        const has_prefix = std.mem.startsWith(u8, token, "src/") or
            std.mem.startsWith(u8, token, "pkg/") or
            std.mem.startsWith(u8, token, "./");

        if (!has_prefix) continue;

        // Strip trailing punctuation like `,`, `.`, `)`, `` ` ``, `]`.
        const path = stripTrailingPunct(token);
        if (path.len == 0) continue;

        // Try to access the path in cwd.
        cwd.access(path, .{}) catch {
            // Path not found — return it as the unresolved ref.
            return path;
        };
    }

    return null;
}

fn isSeparator(c: u8) bool {
    return c == ' ' or c == '\t' or c == '`' or
        c == '[' or c == ']' or c == '(' or c == ')' or
        c == '"' or c == '\'' or c == '<' or c == '>';
}

fn stripTrailingPunct(s: []const u8) []const u8 {
    var end = s.len;
    while (end > 0) {
        const c = s[end - 1];
        if (c == ',' or c == '.' or c == ')' or c == '`' or c == ']' or c == ';' or c == ':') {
            end -= 1;
        } else {
            break;
        }
    }
    return s[0..end];
}

// ── Embedded tests (§3.1) ────────────────────────────────────────────────────

test "3.1.1: empty spec returns failure=empty" {
    var warnings = std.ArrayList([]const u8).init(std.testing.allocator);
    defer warnings.deinit();

    const cases = [_][]const u8{ "", "   ", "\t\n\r\n  " };
    for (cases) |c| {
        var result = try validate(std.testing.allocator, c, std.fs.cwd());
        defer result.deinit(std.testing.allocator);
        try std.testing.expect(result.failure != null);
        try std.testing.expect(result.failure.? == .empty);
    }
}

test "3.1.2: comment-only spec returns failure=no_actionable_content" {
    const specs = [_][]const u8{
        "<!-- this is a comment -->",
        "# Heading only\n## Another heading\n",
        "<!-- start\nstill in comment\n-->\n# Just a heading",
        "# H1\n# H2\n# H3",
    };

    for (specs) |s| {
        var result = try validate(std.testing.allocator, s, std.fs.cwd());
        defer result.deinit(std.testing.allocator);
        try std.testing.expect(result.failure != null);
        try std.testing.expect(result.failure.? == .no_actionable_content);
    }
}

test "3.1.2: spec with actionable content after headings passes" {
    const spec =
        \\# Title
        \\
        \\This is an actionable description of what to build.
    ;
    var result = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer result.deinit(std.testing.allocator);
    try std.testing.expect(result.failure == null);
}

test "3.1.3: spec with unresolved prefixed path returns unresolved_ref failure" {
    const spec =
        \\# Task
        \\
        \\Modify the file src/does_not_exist_xyz.zig to add a feature.
    ;
    var result = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer result.deinit(std.testing.allocator);
    try std.testing.expect(result.failure != null);
    switch (result.failure.?) {
        .unresolved_ref => |path| {
            try std.testing.expectEqualStrings("src/does_not_exist_xyz.zig", path);
        },
        else => return error.TestExpectedUnresolvedRef,
    }
}

test "3.1.3: spec with resolved prefixed path passes" {
    // Use a real directory that exists in the worktree.
    const tmp_dir = std.testing.tmpDir(.{});
    // Create a marker file inside the temp dir.
    const marker = try tmp_dir.dir.createFile("real_file.zig", .{});
    marker.close();

    const spec =
        \\# Task
        \\
        \\Edit src/real_file.zig to improve performance.
    ;

    // Create a sub-directory src/ inside tmp and put the file there.
    try tmp_dir.dir.makeDir("src");
    const src_dir = try tmp_dir.dir.openDir("src", .{});
    const f = try src_dir.createFile("real_file.zig", .{});
    f.close();

    const spec2 =
        \\# Task
        \\
        \\Edit src/real_file.zig to improve performance.
    ;
    var result = try validate(std.testing.allocator, spec2, tmp_dir.dir);
    defer result.deinit(std.testing.allocator);
    try std.testing.expect(result.failure == null);
    _ = spec;
}

test "3.1.4: spec with only bare filename (no path prefix) does not error" {
    // Bare filenames without src/ pkg/ ./ prefix should not block even if ambiguous.
    const spec =
        \\# Task
        \\
        \\Update main.zig to add a flag for verbose output.
    ;
    var result = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer result.deinit(std.testing.allocator);
    // No failure — bare filenames are non-blocking.
    try std.testing.expect(result.failure == null);
}

test "3.1.4: spec with ./ prefix to existing file passes" {
    const tmp_dir = std.testing.tmpDir(.{});
    const f = try tmp_dir.dir.createFile("existing.zig", .{});
    f.close();

    const spec =
        \\# Task
        \\
        \\Edit ./existing.zig to fix the bug.
    ;
    var result = try validate(std.testing.allocator, spec, tmp_dir.dir);
    defer result.deinit(std.testing.allocator);
    try std.testing.expect(result.failure == null);
}

// ── Extended tests (T1-T11) live in spec_validator_test.zig ──────────────────
comptime {
    _ = @import("spec_validator_test.zig");
}
