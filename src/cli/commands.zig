const std = @import("std");

const Subcommand = enum {
    serve,
    worker,
    doctor,
    migrate,
    reconcile,
};

/// Returns null for unknown tokens so the caller can fail loudly.
/// Silent fallback to `.serve` would let a stale `zombied run` invocation
/// start the HTTP server instead of erroring out.
fn parseSubcommandName(name: []const u8) ?Subcommand {
    return std.meta.stringToEnum(Subcommand, name);
}

/// Pure helper — resolves a subcommand from a pre-collected argv slice.
/// Extracted from `parseSubcommandFromProcessArgs` so tests can exercise
/// both the "no subcommand → default" path and the unknown-token error.
fn parseSubcommandFromArgv(argv: []const []const u8) !Subcommand {
    if (argv.len <= 1) return .serve;
    return parseSubcommandName(argv[1]) orelse error.UnknownSubcommand;
}

/// Parse argv[1] into a Subcommand. Returns `.serve` when argv has no
/// subcommand (the historical default), or an error when the given token
/// is not a known subcommand.
pub fn parseSubcommandFromProcessArgs() !Subcommand {
    var args = std.process.args();
    _ = args.next();
    const cmd = args.next() orelse return .serve;
    return parseSubcommandName(cmd) orelse error.UnknownSubcommand;
}

test "parseSubcommandName returns known subcommands" {
    try std.testing.expectEqual(@as(?Subcommand, .serve), parseSubcommandName("serve"));
    try std.testing.expectEqual(@as(?Subcommand, .worker), parseSubcommandName("worker"));
    try std.testing.expectEqual(@as(?Subcommand, .doctor), parseSubcommandName("doctor"));
    try std.testing.expectEqual(@as(?Subcommand, .migrate), parseSubcommandName("migrate"));
    try std.testing.expectEqual(@as(?Subcommand, .reconcile), parseSubcommandName("reconcile"));
}

test "parseSubcommandName returns null for unknown values" {
    try std.testing.expectEqual(@as(?Subcommand, null), parseSubcommandName("unknown"));
    try std.testing.expectEqual(@as(?Subcommand, null), parseSubcommandName("help"));
    // `run` must not silently map to any real subcommand.
    try std.testing.expectEqual(@as(?Subcommand, null), parseSubcommandName("run"));
}

test "parseSubcommandFromArgv defaults to serve when argv has no subcommand" {
    // T1/T2: empty and binary-only argv both map to the historical default.
    try std.testing.expectEqual(Subcommand.serve, try parseSubcommandFromArgv(&.{}));
    try std.testing.expectEqual(Subcommand.serve, try parseSubcommandFromArgv(&.{"zombied"}));
}

test "parseSubcommandFromArgv resolves each known subcommand" {
    // T1: happy path — each variant the dispatcher handles must round-trip.
    const cases = [_]struct { arg: []const u8, expected: Subcommand }{
        .{ .arg = "serve", .expected = .serve },
        .{ .arg = "worker", .expected = .worker },
        .{ .arg = "doctor", .expected = .doctor },
        .{ .arg = "migrate", .expected = .migrate },
        .{ .arg = "reconcile", .expected = .reconcile },
    };
    for (cases) |c| {
        try std.testing.expectEqual(c.expected, try parseSubcommandFromArgv(&.{ "zombied", c.arg }));
    }
}

test "parseSubcommandFromArgv returns UnknownSubcommand error for removed and unknown tokens" {
    // T3: negative path — removed subcommands (run, spec-validate) and typos
    // must surface as an error so the dispatcher can exit 1 instead of
    // silently falling through to `.serve` and booting the HTTP server.
    const rejects = [_][]const u8{ "run", "spec-validate", "unknown", "help", "" };
    for (rejects) |bad| {
        try std.testing.expectError(
            error.UnknownSubcommand,
            parseSubcommandFromArgv(&.{ "zombied", bad }),
        );
    }
}

test "parseSubcommandFromArgv ignores trailing arguments past the subcommand slot" {
    // T2 edge: `zombied run --watch foo.yaml` (the exact legacy shape) must
    // still error on the subcommand slot — extra argv entries don't rescue it.
    try std.testing.expectError(
        error.UnknownSubcommand,
        parseSubcommandFromArgv(&.{ "zombied", "run", "--watch", "foo.yaml" }),
    );
    // And extra args after a valid subcommand don't change the parse result.
    try std.testing.expectEqual(
        Subcommand.serve,
        try parseSubcommandFromArgv(&.{ "zombied", "serve", "--port", "8080" }),
    );
}

test "Subcommand enum has no run variant" {
    const fields = @typeInfo(Subcommand).@"enum".fields;
    comptime var has_run = false;
    comptime {
        for (fields) |f| {
            if (std.mem.eql(u8, f.name, "run")) has_run = true;
        }
    }
    try std.testing.expect(!has_run);
    try std.testing.expectEqual(@as(usize, 5), fields.len);
}
