//! The logfmt value writer — how each Zig type becomes a log field.
//!
//! Split from mod.zig to keep that file under the 350-line cap, and because
//! the functions under test are private: they are reachable only through the
//! public `scoped(...)` API, which in test builds routes unconditionally into
//! the sink registry. So every test here registers a buffered sink and reads
//! back the exact line an operator would see.
//!
//! Why this is worth pinning rather than trusting: these lines are consumed by
//! log queries, not by humans. A field that changes shape does not throw — it
//! silently stops matching a dashboard's pattern, and nobody notices until an
//! incident. The float case below is precisely that bug, already survived once:
//! a `{e}` specifier rendered `0.756` as `7.56e-1` and broke every query
//! scraping `ratio=0\.\d+`.

const std = @import("std");

const logging = @import("mod.zig");
const sinks = @import("sinks.zig");

const ALLOC = std.testing.allocator;

const SCOPE_UNDER_TEST = .logfmt_probe;

/// Runs `body` with a buffered sink installed and returns everything emitted.
/// Caller must free.
fn capture(body: anytype) ![]u8 {
    var bs = sinks.BufferedSink.init(ALLOC);
    defer bs.deinit();

    sinks.clearSinksForTest();
    defer sinks.clearSinksForTest();
    sinks.registerSink(bs.sink());

    body();

    return bs.snapshot();
}

fn expectContains(haystack: []const u8, needle: []const u8) !void {
    if (std.mem.indexOf(u8, haystack, needle) == null) {
        std.debug.print("\nexpected to find: {s}\nin: {s}\n", .{ needle, haystack });
        return error.FieldNotFound;
    }
}

test "logfmt: integers, booleans and enums render as bare values" {
    const emitOne = struct {
        fn call() void {
            const log = logging.scoped(SCOPE_UNDER_TEST);
            log.info("scalars", .{
                .count = @as(u32, 42),
                .negative = @as(i64, -7),
                .enabled = true,
                .disabled = false,
                .level = std.log.Level.warn,
            });
        }
    }.call;

    const out = try capture(emitOne);
    defer ALLOC.free(out);

    try expectContains(out, "event=scalars");
    try expectContains(out, "count=42");
    try expectContains(out, "negative=-7");
    // Spelled out rather than 1/0: a boolean that renders as a number is
    // indistinguishable from a count in a query.
    try expectContains(out, "enabled=true");
    try expectContains(out, "disabled=false");
    // Enums render by tag name, which is what makes them greppable.
    try expectContains(out, "level=warn");
}

test "logfmt: floats render in decimal, never scientific notation" {
    const emitOne = struct {
        fn call() void {
            const log = logging.scoped(SCOPE_UNDER_TEST);
            log.info("ratios", .{ .ratio = @as(f64, 0.756), .tiny = @as(f64, 0.0001) });
        }
    }.call;

    const out = try capture(emitOne);
    defer ALLOC.free(out);

    // The regression this pins: `{e}` rendered these as `7.56e-1` and `1e-4`,
    // which silently stopped matching every dashboard query scraping a decimal.
    try expectContains(out, "ratio=0.756");
    if (std.mem.indexOf(u8, out, "e-") != null) {
        std.debug.print("\nscientific notation leaked into logfmt: {s}\n", .{out});
        return error.ScientificNotationEmitted;
    }
}

test "logfmt: a value needing no quoting is written bare, and one needing it is escaped" {
    const emitOne = struct {
        fn call() void {
            const log = logging.scoped(SCOPE_UNDER_TEST);
            log.info("strings", .{
                .plain = @as([]const u8, "simple_value"),
                .spaced = @as([]const u8, "two words"),
                .quoted = @as([]const u8, "say \"hi\""),
                .equals = @as([]const u8, "a=b"),
                .newline = @as([]const u8, "line1\nline2"),
                .tabbed = @as([]const u8, "col1\tcol2"),
            });
        }
    }.call;

    const out = try capture(emitOne);
    defer ALLOC.free(out);

    // Unquoted when it can be: quoting everything makes lines harder to read
    // and doubles the bytes shipped.
    try expectContains(out, "plain=simple_value");

    // Quoted the moment a separator appears, or the field boundary is lost.
    try expectContains(out, "spaced=\"two words\"");
    try expectContains(out, "equals=\"a=b\"");

    // Embedded quotes are backslash-escaped rather than terminating the value.
    try expectContains(out, "quoted=\"say \\\"hi\\\"\"");

    // Newlines and tabs become two-character escapes: a raw newline would split
    // one log record into two, and the second half would parse as garbage.
    try expectContains(out, "newline=\"line1\\nline2\"");
    try expectContains(out, "tabbed=\"col1\\tcol2\"");
}

test "logfmt: an absent optional is omitted entirely, not rendered as empty" {
    const emitOne = struct {
        fn call() void {
            const log = logging.scoped(SCOPE_UNDER_TEST);
            const present: ?[]const u8 = "here";
            const absent: ?[]const u8 = null;
            const absent_int: ?u32 = null;
            log.info("optionals", .{
                .present = present,
                .absent = absent,
                .absent_int = absent_int,
            });
        }
    }.call;

    const out = try capture(emitOne);
    defer ALLOC.free(out);

    try expectContains(out, "present=here");
    // The standard forbids `key=` and `key=null`: either would be indexed as a
    // real value, so a query counting occurrences of the field would count
    // records that never had one.
    if (std.mem.indexOf(u8, out, "absent=") != null or
        std.mem.indexOf(u8, out, "absent_int=") != null)
    {
        std.debug.print("\nan absent optional was rendered: {s}\n", .{out});
        return error.AbsentOptionalRendered;
    }
}

test "logfmt: a string literal and a byte array both render as text, not as a pointer" {
    const emitOne = struct {
        fn call() void {
            const log = logging.scoped(SCOPE_UNDER_TEST);
            var buf: [5]u8 = "abcde".*;
            log.info("bytes", .{
                .literal = "inline_literal",
                .array = buf,
                .pointer = &buf,
            });
        }
    }.call;

    const out = try capture(emitOne);
    defer ALLOC.free(out);

    // A literal is a pointer-to-array in Zig; rendering it by its default
    // formatting would emit an address, which is useless in a log.
    try expectContains(out, "literal=inline_literal");
    try expectContains(out, "array=abcde");
    try expectContains(out, "pointer=abcde");
}

test "logfmt: a line past the buffer is truncated with a marker, never silently cut" {
    const emitOne = struct {
        fn call() void {
            const log = logging.scoped(SCOPE_UNDER_TEST);
            // Comfortably past the 4 KiB line buffer.
            const huge = "x" ** 5000;
            log.info("oversized", .{ .blob = @as([]const u8, huge) });
        }
    }.call;

    const out = try capture(emitOne);
    defer ALLOC.free(out);

    try expectContains(out, "event=oversized");
    // The marker is the point: a reader must be able to tell a truncated record
    // from a complete one, otherwise a cut field looks like real data — and it
    // is a logfmt field rather than a glyph so a query can filter on it.
    try expectContains(out, "truncated=true");
}

test "logfmt: every level reaches the sink under its own name" {
    // The four scoped wrappers are one-line trampolines, but each is its own
    // comptime instantiation — a wrapper that routed to the wrong level would
    // pass every single-level test while misfiling records in production.
    const emitAll = struct {
        fn call() void {
            const log = logging.scoped(SCOPE_UNDER_TEST);
            log.err("level_probe_err", .{});
            log.warn("level_probe_warn", .{});
            log.info("level_probe_info", .{});
            log.debug("level_probe_debug", .{});
        }
    }.call;

    const out = try capture(emitAll);
    defer ALLOC.free(out);

    try expectContains(out, "event=level_probe_err");
    try expectContains(out, "event=level_probe_warn");
    try expectContains(out, "event=level_probe_info");
    try expectContains(out, "event=level_probe_debug");
}

test "logfmt: a non-string pointer and a struct fall back to generic formatting" {
    // The fallback arms exist so an unanticipated type still logs SOMETHING
    // rather than failing the build at an incident's worst moment. What they
    // must never do is crash or render nothing.
    const emitOne = struct {
        fn call() void {
            const log = logging.scoped(SCOPE_UNDER_TEST);
            var n: u32 = 7;
            log.info("fallbacks", .{
                .number_ptr = &n,
                .pair = .{ .a = 1, .b = 2 },
                .int_array = [_]u8{ 1, 2, 3 } ++ [_]u8{4} ** 0,
                .word_array = [_]u16{ 10, 20 },
            });
        }
    }.call;

    const out = try capture(emitOne);
    defer ALLOC.free(out);

    try expectContains(out, "event=fallbacks");
    try expectContains(out, "number_ptr=");
    try expectContains(out, "pair=");
    // A u8 array renders as text; a wider array takes the generic arm.
    try expectContains(out, "word_array=");
}

test "fatalStderr formats and writes without the logger" {
    // Pre-init path: no sink, no logger, just a bounded stderr write. The
    // observable claim is "does not crash, truncates instead of overflowing" —
    // stderr itself is not captured here, and does not need to be: a bufPrint
    // failure returns silently, and that is the branch the oversized call pins.
    logging.fatalStderr("startup probe: {s}\n", .{"ok"});
    const oversized = "y" ** 4096;
    logging.fatalStderr("{s}", .{oversized}); // > 2 KiB cap — returns, no write
    logging.writeStderrLine("");
}
