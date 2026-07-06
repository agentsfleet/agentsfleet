//! Mechanical generator for docs/api-reference/error-codes.mdx (own repo:
//! ~/Projects/docs). `make gen-error-codes` runs this binary and redirects
//! stdout to that file. Grouping is purely by the code's own category token
//! (`UZ-<CAT>-<NNN>`) in first-seen REGISTRY order; no hand-curated prose
//! survives a regeneration — `title`/`hint` ARE the "Title"/"Common Causes"
//! columns. That trade (mechanical accuracy over curated polish) was an
//! explicit call, not a default — see the spec's Discovery log for the reasoning.
const std = @import("std");
// Imports the two entry tables directly (not error_registry.zig) — that file's
// aggregator `test {}` block reaches outside errors/ (e.g. ../http/handlers/
// common.zig), which would violate this exe's narrower module root.
const entries = @import("error_entries.zig");
const entries_runtime = @import("error_entries_runtime.zig");
const Entry = entries.Entry;
const REGISTRY = entries.ENTRIES ++ entries_runtime.ENTRIES_RUNTIME;
const common = @import("common");

const PRELUDE =
    \\---
    \\title: 'Error Codes'
    \\description: 'All error responses from agentsfleetd use RFC 7807 (application/problem+json). This page lists every error code, its HTTP status, and common causes.'
    \\---
    \\
    \\## Response format
    \\
    \\Every `4xx` and `5xx` response uses `Content-Type: application/problem+json`:
    \\
    \\```json
    \\{
    \\  "docs_uri": "https://docs.agentsfleet.net/api-reference/error-codes#UZ-AGT-009",
    \\  "title": "Fleet not found",
    \\  "detail": "No fleet with id '0198a7ba-2c1d-7f08-8a45-3e9b6d2f1c70' in this workspace.",
    \\  "error_code": "UZ-AGT-009",
    \\  "request_id": "0198a7b5-3c8d-7e41-9a2b-6f1d4c7e8a05"
    \\}
    \\```
    \\
    \\| Field | Description |
    \\|---|---|
    \\| `docs_uri` | Stable link to this page for the specific code |
    \\| `title` | Short label — identical for every occurrence of a given code |
    \\| `detail` | Instance-specific context (varies per call) |
    \\| `error_code` | Machine-readable code. Use this for programmatic handling. |
    \\| `request_id` | Correlation ID for support and log tracing |
    \\
    \\<Note>
    \\This page is generated from agentsfleetd's error registry (`make gen-error-codes`) — every code below is one the server can actually emit. A retired code disappears from this page entirely; an old `docs_uri` anchor that 404s here means the code was removed.
    \\</Note>
    \\
;

fn categoryOf(code: []const u8) []const u8 {
    const rest = code["UZ-".len..];
    const dash = std.mem.indexOfScalar(u8, rest, '-').?;
    return rest[0..dash];
}

fn writeCategoryHeading(w: *std.Io.Writer, cat: []const u8) !void {
    try w.writeByte('#');
    try w.writeByte('#');
    try w.writeByte(' ');
    try w.writeByte(std.ascii.toUpper(cat[0]));
    for (cat[1..]) |c| try w.writeByte(std.ascii.toLower(c));
    try w.writeByte('\n');
}

/// Pure render: REGISTRY -> mdx text on `w`. Exposed so a test can prove
/// idempotency without touching stdout or the docs-repo file.
pub fn render(alloc: std.mem.Allocator, w: *std.Io.Writer) !void {
    var order: std.ArrayList([]const u8) = .empty;
    defer order.deinit(alloc);
    var groups: std.ArrayList(std.ArrayList(Entry)) = .empty;
    defer {
        for (groups.items) |*g| g.deinit(alloc);
        groups.deinit(alloc);
    }

    for (REGISTRY) |entry| {
        const cat = categoryOf(entry.code);
        var idx: ?usize = null;
        for (order.items, 0..) |c, i| {
            if (std.mem.eql(u8, c, cat)) {
                idx = i;
                break;
            }
        }
        const group_idx = idx orelse blk: {
            try order.append(alloc, cat);
            try groups.append(alloc, .empty);
            break :blk order.items.len - 1;
        };
        try groups.items[group_idx].append(alloc, entry);
    }

    try w.writeAll(PRELUDE);
    for (order.items, 0..) |cat, i| {
        try w.writeByte('\n');
        try writeCategoryHeading(w, cat);
        try w.writeAll("\n| Code | HTTP | Title | Common Causes |\n|---|---|---|---|\n");
        for (groups.items[i].items) |e| {
            try w.print("| `{s}` | {d} | {s} | {s} |\n", .{ e.code, @intFromEnum(e.http_status), e.title, e.hint });
        }
    }
}

pub fn main() !void {
    var gpa: std.heap.DebugAllocator(.{}) = .init;
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();
    const io = common.globalIo();

    var stdout_buf: [16384]u8 = undefined;
    var stdout_w = std.Io.File.stdout().writer(io, &stdout_buf);
    const w = &stdout_w.interface;

    try render(alloc, w);
    try w.flush();
}
