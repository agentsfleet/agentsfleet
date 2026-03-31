/// M16_002 §3.1 extended tests — T1-T11 multi-tier coverage.
/// Imported by spec_validator.zig via comptime.
const std = @import("std");
const sv = @import("spec_validator.zig");
const validate = sv.validate;
const stripTrailingPunct = sv.stripTrailingPunct;
const isSeparator = sv.isSeparator;

// ── T1: Boundary — whitespace-only variants ──────────────────────────────────

test "T1: empty string triggers empty failure" {
    var r = try validate(std.testing.allocator, "", std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure.? == .empty);
}

test "T1: single space triggers empty failure" {
    var r = try validate(std.testing.allocator, " ", std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure.? == .empty);
}

test "T1: only LF newlines triggers empty failure" {
    var r = try validate(std.testing.allocator, "\n\n\n", std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure.? == .empty);
}

test "T1: only CRLF triggers empty failure" {
    var r = try validate(std.testing.allocator, "\r\n\r\n", std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure.? == .empty);
}

test "T1: only tabs triggers empty failure" {
    var r = try validate(std.testing.allocator, "\t\t\t", std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure.? == .empty);
}

test "T1: 512-char whitespace blob triggers empty failure" {
    const ws = " " ** 512;
    var r = try validate(std.testing.allocator, ws, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure.? == .empty);
}

// ── T2: Heading-only variants → no_actionable_content ────────────────────────

test "T2: single # heading with no body" {
    var r = try validate(std.testing.allocator, "#\n", std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure.? == .no_actionable_content);
}

test "T2: h6 heading with no body" {
    var r = try validate(std.testing.allocator, "###### Deep heading\n", std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure.? == .no_actionable_content);
}

test "T2: heading with trailing spaces and no body" {
    var r = try validate(std.testing.allocator, "# Title   \n   \n", std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure.? == .no_actionable_content);
}

test "T2: multiple heading levels no body" {
    const spec = "# H1\n## H2\n### H3\n#### H4\n";
    var r = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure.? == .no_actionable_content);
}

// ── T3: Comment variants ──────────────────────────────────────────────────────

test "T3: single-line HTML comment is not actionable" {
    var r = try validate(std.testing.allocator, "<!-- x -->\n", std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure.? == .no_actionable_content);
}

test "T3: multi-line comment spanning lines is not actionable" {
    const spec = "<!--\nstill inside\nclose -->  \n# Heading\n";
    var r = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure.? == .no_actionable_content);
}

test "T3: comment with nested > chars does not confuse parser" {
    const spec = "<!-- NOTE: use > operator -->\n# Title\n";
    var r = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure.? == .no_actionable_content);
}

test "T3: actionable line before comment still passes" {
    const spec = "Do this important task.\n<!-- ignore me -->\n";
    var r = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure == null);
}

test "T3: actionable line after multi-line comment passes" {
    const spec = "<!--\nhidden\n-->\n\nActually do this.\n";
    var r = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure == null);
}

// ── T4: Actionable content detection ─────────────────────────────────────────

test "T4: single word line is actionable" {
    var r = try validate(std.testing.allocator, "refactor\n", std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure == null);
}

test "T4: dash list item is actionable" {
    var r = try validate(std.testing.allocator, "# Plan\n\n- step one\n", std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure == null);
}

test "T4: numbered list item is actionable" {
    var r = try validate(std.testing.allocator, "1. First step\n", std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure == null);
}

test "T4: bold markdown text is actionable" {
    var r = try validate(std.testing.allocator, "**Fix the login bug**\n", std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure == null);
}

test "T4: mix of heading + one actionable line passes" {
    const spec = "# Title\n\nDo the work.\n";
    var r = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure == null);
}

// ── T5: File reference prefix variants ───────────────────────────────────────

test "T5: src/ path in backticks is detected and checked" {
    const tmp = std.testing.tmpDir(.{});
    try tmp.dir.makeDir("src");
    var src_h = try tmp.dir.openDir("src", .{});
    defer src_h.close();
    const f = try src_h.createFile("foo.zig", .{});
    f.close();

    const spec = "Edit `src/foo.zig` to fix the issue.\n";
    var r = try validate(std.testing.allocator, spec, tmp.dir);
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure == null);
}

test "T5: src/ path in brackets is detected and checked" {
    const tmp = std.testing.tmpDir(.{});
    try tmp.dir.makeDir("src");
    var src_h = try tmp.dir.openDir("src", .{});
    defer src_h.close();
    const f = try src_h.createFile("bar.zig", .{});
    f.close();

    const spec = "See [src/bar.zig] for details.\n";
    var r = try validate(std.testing.allocator, spec, tmp.dir);
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure == null);
}

test "T5: trailing period stripped before stat" {
    const tmp = std.testing.tmpDir(.{});
    try tmp.dir.makeDir("src");
    var src_h = try tmp.dir.openDir("src", .{});
    defer src_h.close();
    const f = try src_h.createFile("util.zig", .{});
    f.close();

    const spec = "Edit src/util.zig.\n";
    var r = try validate(std.testing.allocator, spec, tmp.dir);
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure == null);
}

test "T5: second path in list is checked after first resolves" {
    const tmp = std.testing.tmpDir(.{});
    try tmp.dir.makeDir("src");
    var src_h = try tmp.dir.openDir("src", .{});
    defer src_h.close();
    const f = try src_h.createFile("a.zig", .{});
    f.close();
    // src/b.zig does not exist → failure on second path
    const spec = "Files: src/a.zig, src/b.zig\n";
    var r = try validate(std.testing.allocator, spec, tmp.dir);
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure.? == .unresolved_ref);
}

test "T5: pkg/ prefix is recognised as requiring resolution" {
    const spec = "Import from pkg/nonexistent_pkg.\n";
    var r = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure.? == .unresolved_ref);
}

// ── T6: Unresolved ref — exact path surfacing ────────────────────────────────

test "T6: unresolved ref names the exact missing path" {
    const spec = "Modify src/missing_file_xyz_abc.zig please.\n";
    var r = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    switch (r.failure.?) {
        .unresolved_ref => |p| try std.testing.expectEqualStrings("src/missing_file_xyz_abc.zig", p),
        else => return error.WrongFailureKind,
    }
}

test "T6: first unresolved path on line is returned" {
    const spec = "Edit src/first_missing_xyz.zig and src/second_missing_xyz.zig.\n";
    var r = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    switch (r.failure.?) {
        .unresolved_ref => |p| try std.testing.expectEqualStrings("src/first_missing_xyz.zig", p),
        else => return error.WrongFailureKind,
    }
}

test "T6: unresolved ref in second paragraph is caught" {
    const spec =
        \\# Task
        \\
        \\First paragraph — no file refs.
        \\
        \\Second paragraph: edit src/no_such_file_ever_xyzzy.zig.
    ;
    var r = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    switch (r.failure.?) {
        .unresolved_ref => |p| try std.testing.expectEqualStrings("src/no_such_file_ever_xyzzy.zig", p),
        else => return error.WrongFailureKind,
    }
}

// ── T7: Bare filename — non-blocking ─────────────────────────────────────────

test "T7: bare .zig filename without prefix does not block" {
    const spec = "Update main.zig to add the flag.\n";
    var r = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure == null);
}

test "T7: bare .md filename does not block" {
    const spec = "See README.md for context.\n";
    var r = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure == null);
}

// ── T9: stripTrailingPunct — helper coverage ──────────────────────────────────

test "T9: stripTrailingPunct strips trailing period" {
    try std.testing.expectEqualStrings("src/foo.zig", stripTrailingPunct("src/foo.zig."));
}

test "T9: stripTrailingPunct strips trailing comma" {
    try std.testing.expectEqualStrings("src/foo.zig", stripTrailingPunct("src/foo.zig,"));
}

test "T9: stripTrailingPunct strips trailing semicolon" {
    try std.testing.expectEqualStrings("src/foo.zig", stripTrailingPunct("src/foo.zig;"));
}

test "T9: stripTrailingPunct strips trailing colon" {
    try std.testing.expectEqualStrings("src/foo.zig", stripTrailingPunct("src/foo.zig:"));
}

test "T9: stripTrailingPunct strips trailing closing paren" {
    try std.testing.expectEqualStrings("src/foo.zig", stripTrailingPunct("src/foo.zig)"));
}

test "T9: stripTrailingPunct strips multiple trailing punct chars" {
    try std.testing.expectEqualStrings("src/foo.zig", stripTrailingPunct("src/foo.zig,."));
}

test "T9: stripTrailingPunct returns empty string on all-punct input" {
    try std.testing.expectEqualStrings("", stripTrailingPunct(".,;:"));
}

test "T9: stripTrailingPunct leaves clean token unchanged" {
    try std.testing.expectEqualStrings("src/foo", stripTrailingPunct("src/foo"));
}

test "T9: stripTrailingPunct handles empty string" {
    try std.testing.expectEqualStrings("", stripTrailingPunct(""));
}

// ── T10: isSeparator — delimiter recognition ──────────────────────────────────

test "T10: isSeparator returns true for all known delimiters" {
    const delims = [_]u8{ ' ', '\t', '`', '[', ']', '(', ')', '"', '\'', '<', '>' };
    for (delims) |c| {
        try std.testing.expect(isSeparator(c));
    }
}

test "T10: isSeparator returns false for alphanumeric and path chars" {
    const not_delims = [_]u8{ 'a', 'Z', '0', '/', '.', '_', '-' };
    for (not_delims) |c| {
        try std.testing.expect(!isSeparator(c));
    }
}

// ── T11: Memory safety — no leaks ────────────────────────────────────────────

test "T11: no leak on empty spec" {
    var r = try validate(std.testing.allocator, "", std.fs.cwd());
    defer r.deinit(std.testing.allocator);
}

test "T11: no leak on valid spec" {
    const spec = "# Title\n\nDo the work.\n";
    var r = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
}

test "T11: no leak on unresolved ref failure" {
    const spec = "Edit src/ghost_file_xyzzy_leak_check.zig.\n";
    var r = try validate(std.testing.allocator, spec, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
}

test "T11: no leak on no_actionable_content failure" {
    var r = try validate(std.testing.allocator, "# Only headings\n", std.fs.cwd());
    defer r.deinit(std.testing.allocator);
}

test "T11: no leak on large spec (32KB)" {
    const large = "Do refactor work.\n" ** 1800;
    var r = try validate(std.testing.allocator, large, std.fs.cwd());
    defer r.deinit(std.testing.allocator);
    try std.testing.expect(r.failure == null);
}
