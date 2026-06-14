const std = @import("std");

const BYTES_PER_KIB = 1024;

pub const CheckResult = struct {
    id: []const u8,
    ok: bool,
    detail: []const u8,
};

pub fn appendCheck(
    alloc: std.mem.Allocator,
    results: *std.ArrayList(CheckResult),
    id: []const u8,
    ok: bool,
    detail: []const u8,
    overall_ok: *bool,
) !void {
    try results.append(alloc, .{
        .id = id,
        .ok = ok,
        .detail = detail,
    });
    if (!ok) overall_ok.* = false;
}

pub fn appendFmtCheck(
    alloc: std.mem.Allocator,
    results: *std.ArrayList(CheckResult),
    id: []const u8,
    ok: bool,
    overall_ok: *bool,
    comptime fmt: []const u8,
    args: anytype,
) !void {
    try appendCheck(alloc, results, id, ok, try std.fmt.allocPrint(alloc, fmt, args), overall_ok);
}

pub fn renderText(stdout: *std.Io.Writer, results: []const CheckResult, overall_ok: bool) !void {
    try stdout.print("agentsfleetd doctor\n\n", .{});
    for (results) |c| {
        try stdout.print("  [{s}] {s}\n", .{
            if (c.ok) "OK" else "FAIL",
            c.detail,
        });
    }
    try stdout.print("\n{s}\n", .{
        if (overall_ok) "All checks passed." else "Some checks failed — fix before running serve.",
    });
}

pub fn renderJson(stdout: *std.Io.Writer, results: []const CheckResult, overall_ok: bool) !void {
    var pass_count: usize = 0;
    for (results) |c| {
        if (c.ok) pass_count += 1;
    }
    const fail_count = results.len - pass_count;

    try stdout.print("{{\"ok\":{s},\"summary\":{{\"total\":{d},\"passed\":{d},\"failed\":{d}}},\"checks\":[", .{
        if (overall_ok) "true" else "false",
        results.len,
        pass_count,
        fail_count,
    });

    for (results, 0..) |c, idx| {
        if (idx > 0) try stdout.print(",", .{});
        try stdout.print("{{\"id\":{f},\"status\":{f},\"detail\":{f}}}", .{
            std.json.fmt(c.id, .{}),
            std.json.fmt(if (c.ok) "ok" else "fail", .{}),
            std.json.fmt(c.detail, .{}),
        });
    }
    try stdout.print("]}}\n", .{});
}

test "dynamic check details stay valid through render with GPA" {
    var gpa = std.heap.DebugAllocator(.{}){};
    defer std.debug.assert(gpa.deinit() == .ok);
    const alloc = gpa.allocator();
    var ok = true;
    var results: std.ArrayList(CheckResult) = .empty;
    defer results.deinit(alloc);
    try appendFmtCheck(alloc, &results, "schema_gate_compat", false, &ok, "schema_gate status=fail expected_versions={d} applied_versions={d} reason_code={s}", .{ 3, 2, "SCHEMA_BEHIND_BINARY" });
    var output_buf: [BYTES_PER_KIB]u8 = undefined;
    var w = std.Io.Writer.fixed(&output_buf);
    try renderJson(&w, results.items, ok);
    const out = w.buffered();
    try std.testing.expect(std.mem.indexOf(u8, out, "\"schema_gate_compat\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, out, "SCHEMA_BEHIND_BINARY") != null);
}
