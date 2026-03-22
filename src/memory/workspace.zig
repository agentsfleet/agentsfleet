//! Workspace memories — cross-run context stored in Postgres.
//! Written by the worker after each Warden run.
//! Injected into Echo's system prompt at the start of each run.

const std = @import("std");
const pg = @import("pg");
const types = @import("../types.zig");
const log = std.log.scoped(.memory);
const id_format = @import("../types/id_format.zig");

/// Fetch recent memories for a workspace, newest first.
/// Returns a formatted string suitable for injection into Echo's prompt.
/// Caller owns the returned slice.
pub fn loadForEcho(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    limit: u32,
) ![]const u8 {
    // check-pg-drain: ok — full while loop exhausts all rows, natural drain
    var result = try conn.query(
        \\SELECT content FROM workspace_memories
        \\WHERE workspace_id = $1
        \\  AND (expires_at IS NULL OR expires_at > $2)
        \\ORDER BY created_at DESC
        \\LIMIT $3
    , .{
        workspace_id,
        std.time.milliTimestamp(),
        @as(i32, @intCast(limit)),
    });
    defer result.deinit();

    var list: std.ArrayList([]const u8) = .{};
    defer list.deinit(alloc);

    while (try result.next()) |row| {
        const content = try row.get([]u8, 0);
        try list.append(alloc, try alloc.dupe(u8, content));
    }

    if (list.items.len == 0) return try alloc.dupe(u8, "");

    var buf: std.ArrayList(u8) = .{};
    try buf.appendSlice(alloc, "## Cross-run workspace memories\n\n");
    for (list.items, 0..) |item, i| {
        defer alloc.free(item);
        try buf.writer(alloc).print("- [{d}] {s}\n", .{ i + 1, item });
    }
    return buf.toOwnedSlice(alloc);
}

/// Save observations from Warden as workspace memories.
/// Each line in `observations` that is non-empty is saved as a separate record.
pub fn saveFromWarden(
    conn: *pg.Conn,
    workspace_id: []const u8,
    run_id: []const u8,
    observations: []const u8,
) !u32 {
    const now_ms = std.time.milliTimestamp();
    // Memories expire after 30 days
    const expire_ms = now_ms + (30 * 24 * 60 * 60 * 1000);

    var saved: u32 = 0;
    var it = std.mem.splitScalar(u8, observations, '\n');
    while (it.next()) |line| {
        const trimmed = std.mem.trim(u8, line, " \t\r");
        if (trimmed.len == 0) continue;
        if (trimmed.len > 2048) continue; // sanity cap

        const memory_id = try id_format.generateWorkspaceMemoryId(conn._allocator);
        defer conn._allocator.free(memory_id);
        _ = try conn.exec(
            \\INSERT INTO workspace_memories
            \\  (id, workspace_id, run_id, content, tags, created_at, expires_at)
            \\VALUES ($1, $2, $3, $4, '[]', $5, $6)
        , .{
            memory_id,
            workspace_id,
            run_id,
            trimmed,
            now_ms,
            expire_ms,
        });
        saved += 1;
    }

    if (saved > 0) log.info("memory.saved count={d} workspace_id={s}", .{ saved, workspace_id });
    return saved;
}
