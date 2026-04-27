//! Minimal JSON string-escaping helpers for the OTLP JSON exporter.
//!
//! OTLP collectors reject malformed JSON silently (network 4xx with no body).
//! String values that reach this module come from Prometheus label values,
//! metric names, and env-var-sourced `service_name` — any of which could
//! contain `"`, `\`, or control characters and break the payload.
//!
//! RFC 8259 §7 compliant: escapes `"`, `\`, control chars `U+0000..U+001F`.
//! Forward slash `/` is *allowed* to appear unescaped in JSON strings per the
//! spec, so we leave it alone. Non-ASCII is passed through verbatim (the JSON
//! grammar allows any Unicode except the escaped set; collectors accept UTF-8).

const std = @import("std");

fn jsonEscapeByte(writer: anytype, c: u8) !void {
    switch (c) {
        '"' => try writer.writeAll("\\\""),
        '\\' => try writer.writeAll("\\\\"),
        '\n' => try writer.writeAll("\\n"),
        '\r' => try writer.writeAll("\\r"),
        '\t' => try writer.writeAll("\\t"),
        0x08 => try writer.writeAll("\\b"),
        0x0C => try writer.writeAll("\\f"),
        0x00...0x07, 0x0B, 0x0E...0x1F => try writer.print("\\u{x:0>4}", .{c}),
        else => try writer.writeByte(c),
    }
}

/// Write `s` as a JSON string *value* (no surrounding quotes).
fn writeEscaped(writer: anytype, s: []const u8) !void {
    for (s) |c| try jsonEscapeByte(writer, c);
}

/// Decode a Prometheus-escaped label value and write it as a JSON-escaped
/// string *value* (no surrounding quotes). Prometheus exposition format
/// escapes `\\`, `\"`, and `\n` inside label values; everything else is a
/// raw byte. Unknown `\X` sequences pass through with the backslash literal
/// retained (tolerating non-standard renderers).
fn writePromValueAsJson(writer: anytype, s: []const u8) !void {
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        const c = s[i];
        if (c == '\\' and i + 1 < s.len) {
            const decoded: u8 = switch (s[i + 1]) {
                '\\' => '\\',
                '"' => '"',
                'n' => '\n',
                else => {
                    try jsonEscapeByte(writer, c);
                    continue;
                },
            };
            try jsonEscapeByte(writer, decoded);
            i += 1;
            continue;
        }
        try jsonEscapeByte(writer, c);
    }
}

/// Return the index of the next `"` at or after `from` that is not preceded
/// by a `\` (i.e. not a Prometheus-escaped quote). Null if no unescaped quote
/// exists in the remainder of the slice.
pub fn findUnescapedQuote(s: []const u8, from: usize) ?usize {
    var i = from;
    while (i < s.len) : (i += 1) {
        if (s[i] == '\\') {
            if (i + 1 < s.len) i += 1; // skip the escaped byte
            continue;
        }
        if (s[i] == '"') return i;
    }
    return null;
}

/// Write `s` as a complete JSON string literal, including the surrounding
/// quotes. Convenience wrapper for the common case.
pub fn writeQuoted(writer: anytype, s: []const u8) !void {
    try writer.writeByte('"');
    try writeEscaped(writer, s);
    try writer.writeByte('"');
}

/// Serialize a Prometheus label set `k1="v1",k2="v2"` as an OTLP attribute
/// array. No-op for unlabeled metrics. Writes a leading comma + `"attributes":[…]`
/// fragment that plugs into an open dataPoints object.
pub fn writeAttributes(writer: anytype, labels_src: []const u8) !void {
    if (labels_src.len == 0) return;
    try writer.writeAll(",\"attributes\":[");
    var rest = labels_src;
    var first = true;
    while (rest.len > 0) {
        const eq = std.mem.indexOfScalar(u8, rest, '=') orelse break;
        const key = std.mem.trim(u8, rest[0..eq], " ,");
        if (eq + 1 >= rest.len or rest[eq + 1] != '"') break;
        const v_start = eq + 2;
        // Respect Prometheus `\"` escapes so a value like `a\"b` isn't
        // truncated at the embedded quote.
        const v_end = findUnescapedQuote(rest, v_start) orelse break;
        const val = rest[v_start..v_end];
        if (!first) try writer.writeAll(",");
        first = false;
        try writer.writeAll("{\"key\":");
        try writeQuoted(writer, key);
        try writer.writeAll(",\"value\":{\"stringValue\":\"");
        try writePromValueAsJson(writer, val);
        try writer.writeAll("\"}}");
        rest = rest[v_end + 1 ..];
        if (rest.len > 0 and rest[0] == ',') rest = rest[1..];
    }
    try writer.writeAll("]");
}

test "writeEscaped passes ASCII through unchanged" {
    var buf: [64]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try writeEscaped(fbs.writer(), "zombie_agent_tokens");
    try std.testing.expectEqualStrings("zombie_agent_tokens", fbs.getWritten());
}

test "writeEscaped handles quote, backslash, newline, tab" {
    var buf: [64]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try writeEscaped(fbs.writer(), "a\"b\\c\nd\te");
    try std.testing.expectEqualStrings("a\\\"b\\\\c\\nd\\te", fbs.getWritten());
}

test "writeEscaped encodes other control chars via \\uXXXX" {
    var buf: [64]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try writeEscaped(fbs.writer(), "\x01\x1f");
    try std.testing.expectEqualStrings("\\u0001\\u001f", fbs.getWritten());
}

test "writeEscaped leaves forward slash and non-ASCII alone" {
    var buf: [64]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try writeEscaped(fbs.writer(), "/path/é");
    try std.testing.expectEqualStrings("/path/é", fbs.getWritten());
}

test "writePromValueAsJson decodes backslash-quote without truncating" {
    var buf: [64]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    // Prometheus-exposition source `a\"b\\c` decodes to literal `a"b\c`,
    // which must JSON-escape to `a\"b\\c` (double-escaped quote + doubled
    // backslash).
    try writePromValueAsJson(fbs.writer(), "a\\\"b\\\\c");
    try std.testing.expectEqualStrings("a\\\"b\\\\c", fbs.getWritten());
}

test "writePromValueAsJson decodes backslash-n to newline then JSON-escapes" {
    var buf: [64]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try writePromValueAsJson(fbs.writer(), "line1\\nline2");
    try std.testing.expectEqualStrings("line1\\nline2", fbs.getWritten());
}

test "findUnescapedQuote skips escaped quotes" {
    // Input bytes: a \ " b \ " c " d (9 bytes) — first unescaped " at idx 7.
    try std.testing.expectEqual(@as(?usize, 7), findUnescapedQuote("a\\\"b\\\"c\"d", 0));
    try std.testing.expectEqual(@as(?usize, null), findUnescapedQuote("no quote here", 0));
}

test "writeQuoted wraps in quotes" {
    var buf: [64]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try writeQuoted(fbs.writer(), "hello \"world\"");
    try std.testing.expectEqualStrings("\"hello \\\"world\\\"\"", fbs.getWritten());
}
