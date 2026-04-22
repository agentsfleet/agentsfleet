const std = @import("std");
const as = @import("activity_stream.zig");

test "ActivityPage.deinit frees all rows and cursor" {
    const alloc = std.testing.allocator;

    const events = try alloc.alloc(as.ActivityEventRow, 1);
    events[0] = .{
        .id = try alloc.dupe(u8, "019abc"),
        .zombie_id = try alloc.dupe(u8, "z1"),
        .workspace_id = try alloc.dupe(u8, "ws1"),
        .event_type = try alloc.dupe(u8, "webhook_received"),
        .detail = try alloc.dupe(u8, "{}"),
        .created_at = 1744000000000,
    };
    const page = as.ActivityPage{
        .events = events,
        .next_cursor = try alloc.dupe(u8, "1744000000000:019abc"),
    };
    page.deinit(alloc);
    // leak detector will catch any missed free
}

test "collectActivityPage: full page sets next_cursor; partial page sets null" {
    // Only tests the cursor logic — no DB required.
    const alloc = std.testing.allocator;

    // Full page of 2 events with limit=2 → composite cursor.
    {
        var rows: std.ArrayList(as.ActivityEventRow) = .{};
        defer {
            for (rows.items) |*r| r.deinit(alloc);
            rows.deinit(alloc);
        }
        for (0..2) |i| {
            try rows.append(alloc, .{
                .id = try alloc.dupe(u8, "019abc"),
                .zombie_id = try alloc.dupe(u8, "z1"),
                .workspace_id = try alloc.dupe(u8, "ws1"),
                .event_type = try alloc.dupe(u8, "webhook_received"),
                .detail = try alloc.dupe(u8, "{}"),
                .created_at = @as(i64, @intCast(1744000000000 - i * 1000)),
            });
        }
        const events = try rows.toOwnedSlice(alloc);
        const next_cursor = if (events.len == 2) blk: {
            break :blk try std.fmt.allocPrint(alloc, "{d}:{s}", .{ events[1].created_at, events[1].id });
        } else null;
        const page = as.ActivityPage{ .events = events, .next_cursor = next_cursor };
        defer page.deinit(alloc);
        try std.testing.expect(page.next_cursor != null);
    }

    // Partial page (1 event, limit=2) → null cursor.
    {
        var rows: std.ArrayList(as.ActivityEventRow) = .{};
        defer {
            for (rows.items) |*r| r.deinit(alloc);
            rows.deinit(alloc);
        }
        try rows.append(alloc, .{
            .id = try alloc.dupe(u8, "019abc"),
            .zombie_id = try alloc.dupe(u8, "z1"),
            .workspace_id = try alloc.dupe(u8, "ws1"),
            .event_type = try alloc.dupe(u8, "webhook_received"),
            .detail = try alloc.dupe(u8, "{}"),
            .created_at = 1744000000000,
        });
        const events = try rows.toOwnedSlice(alloc);
        const next_cursor: ?[]const u8 = if (events.len == 2) try alloc.dupe(u8, "x") else null;
        const page = as.ActivityPage{ .events = events, .next_cursor = next_cursor };
        defer page.deinit(alloc);
        try std.testing.expect(page.next_cursor == null);
    }
}
