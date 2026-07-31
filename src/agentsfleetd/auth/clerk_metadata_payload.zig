//! Clerk metadata-merge payload rendering, split from clerk_backend.zig by
//! concern (and the file cap). Pure — no HTTP, no allocator beyond the
//! result — so the payload shape is testable without a client.

const std = @import("std");

pub const RenderError = error{ SerializationFailed, OutOfMemory };

/// Render `{"public_metadata":{...}}` with the present fields; caller must
/// free the returned slice.
pub fn renderMetadataPayload(
    alloc: std.mem.Allocator,
    tenant_id: ?[]const u8,
    scopes: ?[]const u8,
) RenderError![]u8 {
    var aw: std.Io.Writer.Allocating = .init(alloc);
    defer aw.deinit();
    const w = &aw.writer;

    w.writeAll("{\"public_metadata\":{") catch return RenderError.SerializationFailed;
    var first = true;
    if (tenant_id) |v| {
        writeJsonKeyValue(w, &first, "tenant_id", v) catch return RenderError.SerializationFailed;
    }
    if (scopes) |v| {
        writeJsonKeyValue(w, &first, "scopes", v) catch return RenderError.SerializationFailed;
    }
    w.writeAll("}}") catch return RenderError.SerializationFailed;
    return aw.toOwnedSlice() catch return RenderError.OutOfMemory;
}

fn writeJsonKeyValue(w: anytype, first: *bool, key: []const u8, value: []const u8) !void {
    if (!first.*) try w.writeAll(",");
    first.* = false;
    try w.writeAll("\"");
    try w.writeAll(key);
    try w.writeAll("\":\"");
    try writeJsonEscaped(w, value);
    try w.writeAll("\"");
}

/// Minimal JSON string-body escaper. Our values are either UUID v7
/// strings (`0195b4ba-…`, no special chars) or the space-delimited scope
/// claim (`"fleet:admin credential:write …"`, ASCII). Escaping `"`, `\`,
/// and ASCII control chars is sufficient — we never pass non-ASCII or
/// Unicode surrogate pairs through this path.
fn writeJsonEscaped(w: anytype, value: []const u8) !void {
    for (value) |c| switch (c) {
        '"' => try w.writeAll("\\\""),
        '\\' => try w.writeAll("\\\\"),
        '\n' => try w.writeAll("\\n"),
        '\r' => try w.writeAll("\\r"),
        '\t' => try w.writeAll("\\t"),
        // All ASCII control bytes outside the explicit \n/\r/\t branches,
        // plus DEL (0x7f). JSON permits bare DEL but downstream log
        // pipelines + operator consoles routinely choke on it, so we
        // escape defensively.
        0x00...0x08, 0x0B, 0x0C, 0x0E...0x1F, 0x7F => {
            var buf: [7]u8 = undefined;
            const hex = std.fmt.bufPrint(&buf, "\\u{x:0>4}", .{c}) catch @panic("bufPrint failed: stack buffer sized incorrectly at compile time");
            try w.writeAll(hex);
        },
        else => try w.writeAll(&[_]u8{c}),
    };
}
