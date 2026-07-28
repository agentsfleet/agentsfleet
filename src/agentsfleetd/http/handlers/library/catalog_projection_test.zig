//! Unit tier for the operator catalog's positional row projection.
//!
//! `catalog.rowToEntry` reads its row by INDEX (`row.get(T, 11)`), and the index
//! is only correct relative to the column order of `SELECT_ADMIN_CATALOG`.
//! Removing a projected column renumbers everything after it — and the failure is
//! silent whenever the neighbour has a compatible type, so the row comes back
//! populated with the wrong field rather than erroring.
//!
//! That is not hypothetical here: dropping the support-file manifest from the
//! projection moved `trigger_present` from 12 to 11 and `updated_at` from 13 to
//! 12. This pins the order so the next such edit fails loudly in CI instead of
//! quietly reporting one column's value under another column's name.

const std = @import("std");

const sql = @import("../../../fleet_library/sql.zig");

const testing = std.testing;

/// Split a SELECT's projection list on top-level commas.
///
/// Paren-depth aware: a column may legitimately contain commas inside a call
/// (`COALESCE(x, y)`), and splitting naively would count one column as two and
/// shift every index after it — the exact class of bug this file exists to catch.
fn projectionColumns(alloc: std.mem.Allocator, statement: []const u8) ![][]const u8 {
    const select_kw = "SELECT ";
    const start = (std.mem.indexOf(u8, statement, select_kw) orelse return error.NoSelect) + select_kw.len;
    const from_kw = "\n  FROM ";
    const end = std.mem.indexOf(u8, statement, from_kw) orelse return error.NoFrom;

    var cols: std.ArrayList([]const u8) = .empty;
    errdefer cols.deinit(alloc);

    var depth: usize = 0;
    var field_start = start;
    var i = start;
    while (i < end) : (i += 1) {
        switch (statement[i]) {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' => if (depth == 0) {
                try cols.append(alloc, std.mem.trim(u8, statement[field_start..i], " \n"));
                field_start = i + 1;
            },
            else => {},
        }
    }
    try cols.append(alloc, std.mem.trim(u8, statement[field_start..end], " \n"));
    return cols.toOwnedSlice(alloc);
}

/// The column order both admin statements project, in the order
/// `catalog.rowToEntry` indexes them. Substrings, not equality: several columns
/// carry a `::text` cast or an alias that is not load-bearing here.
const EXPECTED_COLUMNS = [_][]const u8{
    "id", // 0
    "name", // 1
    "description", // 2
    "source_repo", // 3
    "source_ref", // 4
    "visibility", // 5
    "content_hash", // 6
    "required_credentials", // 7
    "required_tools", // 8
    "network_hosts", // 9
    "required_credentials_reasons", // 10
    "trigger_markdown IS NOT NULL", // 11
    "updated_at", // 12
};

fn expectProjectionOrder(statement: []const u8) !void {
    const alloc = testing.allocator;
    const cols = try projectionColumns(alloc, statement);
    defer alloc.free(cols);

    try testing.expectEqual(EXPECTED_COLUMNS.len, cols.len);
    for (EXPECTED_COLUMNS, cols, 0..) |want, got, index| {
        testing.expect(std.mem.indexOf(u8, got, want) != null) catch {
            std.log.warn("column {d}: expected {s}, projection has {s}", .{ index, want, got });
            return error.ProjectionOrderChanged;
        };
    }
}

test "test_catalog_row_projection_indices_hold" {
    // Dimension 5.3.
    try expectProjectionOrder(sql.SELECT_ADMIN_CATALOG);
}

test "test_catalog_row_projection_indices_hold: the single-row read matches the list" {
    // `SELECT_ADMIN_CATALOG_ROW` decodes through the SAME mapper, so a divergence
    // between the two is a wrong-field read on exactly one of the two routes —
    // the harder kind to notice, because the other route stays correct.
    try expectProjectionOrder(sql.SELECT_ADMIN_CATALOG_ROW);
}

test "test_catalog_row_projection_indices_hold: the manifest is absent from both reads" {
    // The projection change this file was written for. Asserted by name so a
    // reinstated column fails here with a sentence rather than as an opaque
    // count mismatch above.
    try testing.expect(std.mem.indexOf(u8, sql.SELECT_ADMIN_CATALOG, "support_files_json") == null);
    try testing.expect(std.mem.indexOf(u8, sql.SELECT_ADMIN_CATALOG_ROW, "support_files_json") == null);
}

test "the column splitter survives a parenthesised column" {
    // Guards the guard: if the splitter miscounted a `COALESCE(a, b)` column the
    // order assertions above would compare shifted lists and could pass or fail
    // for reasons unrelated to the projection.
    const alloc = testing.allocator;
    const stmt =
        \\SELECT id, COALESCE(a, b), (c IS NOT NULL), d
        \\  FROM t
    ;
    const cols = try projectionColumns(alloc, stmt);
    defer alloc.free(cols);
    try testing.expectEqual(@as(usize, 4), cols.len);
    try testing.expectEqualStrings("COALESCE(a, b)", cols[1]);
    try testing.expectEqualStrings("(c IS NOT NULL)", cols[2]);
}
