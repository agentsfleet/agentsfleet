//! Build and insert one bounded page of verifier intents.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;

const id_format = @import("../types/id_format.zig");
const sql = @import("repair_sql.zig");

pub const Candidate = struct {
    production_result_id: []u8,
    verifier_fleet_id: []u8,
    verify_after: i64,

    pub fn deinit(self: *Candidate, alloc: std.mem.Allocator) void {
        alloc.free(self.production_result_id);
        alloc.free(self.verifier_fleet_id);
    }
};

const Attempt = struct {
    id: []const u8,
    production_result_id: []const u8,
    repair_link_id: []const u8,
    verifier_fleet_id: []const u8,
    verify_after: i64,
};

pub fn insert(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    repair_link_id: []const u8,
    candidates: []const Candidate,
) !usize {
    if (candidates.len == 0) return 0;
    const attempts = try alloc.alloc(Attempt, candidates.len);
    var initialized: usize = 0;
    defer {
        for (attempts[0..initialized]) |attempt| alloc.free(attempt.id);
        alloc.free(attempts);
    }
    for (candidates, 0..) |candidate, index| {
        const id = try id_format.generateRepairVerificationId(alloc);
        attempts[index] = .{
            .id = id,
            .production_result_id = candidate.production_result_id,
            .repair_link_id = repair_link_id,
            .verifier_fleet_id = candidate.verifier_fleet_id,
            .verify_after = candidate.verify_after,
        };
        initialized += 1;
    }
    const payload = try std.json.Stringify.valueAlloc(alloc, attempts, .{});
    defer alloc.free(payload);
    const now_ms = clock.nowMillis();
    const affected = try conn.exec(sql.INSERT_REPAIR_VERIFICATIONS, .{ workspace_id, payload, now_ms });
    return @intCast(affected orelse 0);
}

test "test_fanout_uses_one_set_based_insert" {
    try std.testing.expect(std.mem.indexOf(u8, sql.INSERT_REPAIR_VERIFICATIONS, "jsonb_to_recordset") != null);
}
