//! Logfmt envelope builder shared by every binary's `logFn`.
//!
//! Splits the per-binary `ts_ms=… level=… scope=… <body>\n` envelope
//! out of `mod.zig` (which builds the body) so the wrapping concern
//! stays in one place and is unit-testable independent of the body
//! pipeline.

const std = @import("std");

/// Build the logfmt envelope into `buf`.
///
/// Defense-in-depth: every byte of `body` is scanned and any literal
/// `\n` / `\r` is rewritten to the two-character escape (`\\n` /
/// `\\r`). The body already arrives logfmt-quoted (writeStringValue
/// quotes any value containing `\n`/`\r`), so this scrub is only
/// relevant if a future caller bypasses the scoped API or a struct
/// field's `{any}` rendering emits a raw newline. Without it, a
/// single `\n` in `body` would split the record into two stderr lines
/// — the second has no envelope and is parsed as an attacker-shaped
/// record by Loki/Vector/etc.
///
/// Returns the slice of `buf` that was filled. Falls back to a
/// truncated slice on overflow (callers are about to write to stderr;
/// truncation is safer than dropping the line).
pub fn writeLogfmtEnvelope(
    buf: []u8,
    ts_ms: i64,
    level_str: []const u8,
    scope_str: []const u8,
    body: []const u8,
) []const u8 {
    var fbs = std.io.fixedBufferStream(buf);
    const w = fbs.writer();
    w.print("ts_ms={d} level={s} scope={s} ", .{ ts_ms, level_str, scope_str }) catch
        return fbs.getWritten();
    for (body) |c| {
        switch (c) {
            '\n' => w.writeAll("\\n") catch return fbs.getWritten(),
            '\r' => w.writeAll("\\r") catch return fbs.getWritten(),
            else => w.writeByte(c) catch return fbs.getWritten(),
        }
    }
    w.writeByte('\n') catch return fbs.getWritten();
    return fbs.getWritten();
}

test "writeLogfmtEnvelope scrubs raw \\n in body to escape sequence" {
    var buf: [256]u8 = undefined;
    // Adversarial body: a stray newline that would otherwise split the
    // record into two stderr lines, the second parsed by Loki/Vector
    // as an attacker-shaped record without ts_ms/level/scope envelope.
    const body = "event=foo path=/etc/\nlevel=err scope=evil event=injected";
    const line = writeLogfmtEnvelope(&buf, 1715004901234, "info", "scope_x", body);
    try std.testing.expectEqual(@as(usize, 1), std.mem.count(u8, line, "\n"));
    try std.testing.expect(std.mem.indexOf(u8, line, "\\n") != null);
    try std.testing.expect(std.mem.startsWith(u8, line, "ts_ms=1715004901234 level=info scope=scope_x "));
}

test "writeLogfmtEnvelope passes clean body through verbatim" {
    var buf: [256]u8 = undefined;
    const body = "event=tool_failed tool=bash error_code=UZ-EXEC-012";
    const line = writeLogfmtEnvelope(&buf, 1, "warn", "exec", body);
    try std.testing.expectEqualStrings(
        "ts_ms=1 level=warn scope=exec event=tool_failed tool=bash error_code=UZ-EXEC-012\n",
        line,
    );
}

test "writeLogfmtEnvelope scrubs raw \\r in body" {
    var buf: [128]u8 = undefined;
    const body = "event=x msg=hello\rworld";
    const line = writeLogfmtEnvelope(&buf, 1, "info", "s", body);
    try std.testing.expect(std.mem.indexOf(u8, line, "\\r") != null);
    try std.testing.expectEqual(@as(usize, 1), std.mem.count(u8, line, "\n"));
}
