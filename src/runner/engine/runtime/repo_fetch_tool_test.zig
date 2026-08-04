//! Tests for the child-side `repo_fetch` tool (M157 §4).
//!
//! These drive the tool exactly as NullClaw does — an argument map in, a
//! `ToolResult` out — with the runner played by a pipe the test writes the
//! reply into. What they pin is the boundary the model actually sees: which
//! arguments are accepted, that a refusal arrives as readable prose rather than
//! a bare failure, and that a success puts a relative path and nothing else
//! into context.

const std = @import("std");
const RepoFetchTool = @import("repo_fetch.zig");
const fetch_request = @import("../repo_fetch_request.zig");
const pipe_proto = @import("../../pipe_proto.zig");

const clock = @import("common").clock;
const testing = std.testing;

const SHA1 = "9a078a7b5c1d4e2f60718293a4b5c6d7e8f90123";
const REPOSITORY = "acme/payments";

/// Two pipes standing in for the lease's duplex: the tool writes its ask into
/// one and reads its reply out of the other.
const Wire = struct {
    channel: fetch_request.Channel,
    parent_read: std.posix.fd_t,
    parent_write: std.posix.fd_t,

    fn init() !Wire {
        const req = try pipe_proto.testOsPipe();
        const resp = try pipe_proto.testOsPipe();
        return .{
            .channel = .{
                .request_fd = req[1],
                .response_fd = resp[0],
                .deadline_ms = clock.nowMillis() + 5_000,
            },
            .parent_read = req[0],
            .parent_write = resp[1],
        };
    }

    fn deinit(self: Wire) void {
        pipe_proto.testOsClose(self.channel.request_fd);
        pipe_proto.testOsClose(self.channel.response_fd);
        pipe_proto.testOsClose(self.parent_read);
        pipe_proto.testOsClose(self.parent_write);
    }

    /// Pre-buffer the runner's reply. The round-trip is synchronous, so the
    /// reply must already be in the pipe before the tool blocks reading it.
    fn reply(self: Wire, resp: fetch_request.PipeResponse) !void {
        const json = try std.json.Stringify.valueAlloc(testing.allocator, resp, .{});
        defer testing.allocator.free(json);
        try pipe_proto.writeFrame(self.parent_write, .repo_fetch_response, json);
    }
};

/// Build the argument map NullClaw would hand `execute`. `std.json.ObjectMap`
/// is unmanaged in Zig 0.16, so the allocator rides every call.
fn argsOf(alloc: std.mem.Allocator, map: *std.json.ObjectMap, pairs: []const [2][]const u8) !void {
    for (pairs) |kv| try map.put(alloc, kv[0], .{ .string = kv[1] });
}

test "a successful fetch puts a relative path — and nothing else — into context" {
    const alloc = testing.allocator;
    const w = try Wire.init();
    defer w.deinit();
    try w.reply(.{ .ok = true, .path = "repo" });

    var map: std.json.ObjectMap = .{};
    defer map.deinit(alloc);
    try argsOf(alloc, &map, &.{ .{ "repository", REPOSITORY }, .{ "commit", SHA1 } });

    var t = RepoFetchTool{ .channel = w.channel };
    const result = try t.execute(alloc, map);
    defer alloc.free(result.output);
    try testing.expect(result.success);
    try testing.expectEqualStrings("repo", result.output);
    try testing.expect(result.error_msg == null);

    // The ask that went up the wire names the repository and the commit, and no
    // workspace or path the model could have steered.
    const out = try pipe_proto.readFrame(alloc, w.parent_read, w.channel.deadline_ms, 4096);
    defer alloc.free(out.frame.payload);
    try testing.expectEqual(pipe_proto.FrameType.repo_fetch_request, out.frame.ftype);
    try testing.expect(std.mem.indexOf(u8, out.frame.payload, REPOSITORY) != null);
    try testing.expect(std.mem.indexOf(u8, out.frame.payload, SHA1) != null);
    try testing.expect(std.mem.indexOf(u8, out.frame.payload, "workspace") == null);
}

test "a refusal reaches the model as readable prose, not a bare failure" {
    // The reason is the whole point: a model told only "refused" retries the
    // same ask, and a model told "outside the fleet's binding" stops.
    const alloc = testing.allocator;
    const w = try Wire.init();
    defer w.deinit();
    const reason = "repository is outside the fleet's binding";
    try w.reply(.{ .ok = false, .reason = reason });

    var map: std.json.ObjectMap = .{};
    defer map.deinit(alloc);
    try argsOf(alloc, &map, &.{ .{ "repository", "otherorg/payments" }, .{ "commit", SHA1 } });

    var t = RepoFetchTool{ .channel = w.channel };
    const result = try t.execute(alloc, map);
    defer if (result.error_msg) |m| alloc.free(m);
    try testing.expect(!result.success);
    try testing.expect(std.mem.indexOf(u8, result.error_msg.?, reason) != null);
}

test "a run with no fetch channel fails closed rather than pretending" {
    // The register-only / no-session path. A fleet that declared `repo_fetch`
    // must get a refusal it can read, never a success against an absent tree.
    const alloc = testing.allocator;
    var map: std.json.ObjectMap = .{};
    defer map.deinit(alloc);
    try argsOf(alloc, &map, &.{ .{ "repository", REPOSITORY }, .{ "commit", SHA1 } });

    var t = RepoFetchTool{ .channel = null };
    const result = try t.execute(alloc, map);
    try testing.expect(!result.success);
    try testing.expect(result.error_msg != null);
}

test "the required arguments are required, and head is not" {
    const alloc = testing.allocator;

    // Missing repository / commit are refused before any frame is written, so a
    // malformed call never reaches the daemon at all.
    {
        var map: std.json.ObjectMap = .{};
        defer map.deinit(alloc);
        try argsOf(alloc, &map, &.{.{ "commit", SHA1 }});
        var t = RepoFetchTool{ .channel = null };
        const r = try t.execute(alloc, map);
        try testing.expect(!r.success);
    }
    {
        var map: std.json.ObjectMap = .{};
        defer map.deinit(alloc);
        try argsOf(alloc, &map, &.{.{ "repository", REPOSITORY }});
        var t = RepoFetchTool{ .channel = null };
        const r = try t.execute(alloc, map);
        try testing.expect(!r.success);
    }

    // An omitted head is the remote's default, not an error — the ask carries
    // an empty string and the daemon reads that as "default head".
    const w = try Wire.init();
    defer w.deinit();
    try w.reply(.{ .ok = true, .path = "repo" });
    var map: std.json.ObjectMap = .{};
    defer map.deinit(alloc);
    try argsOf(alloc, &map, &.{ .{ "repository", REPOSITORY }, .{ "commit", SHA1 } });
    var t = RepoFetchTool{ .channel = w.channel };
    const r = try t.execute(alloc, map);
    defer alloc.free(r.output);
    try testing.expect(r.success);
}

test "the declared parameter schema matches the arguments execute reads" {
    // A schema that named a field `execute` ignores would have the model send
    // something silently dropped, which is worse than a refusal.
    try testing.expect(std.mem.indexOf(u8, RepoFetchTool.tool_params, "\"repository\"") != null);
    try testing.expect(std.mem.indexOf(u8, RepoFetchTool.tool_params, "\"commit\"") != null);
    try testing.expect(std.mem.indexOf(u8, RepoFetchTool.tool_params, "\"head\"") != null);
    try testing.expect(std.mem.indexOf(u8, RepoFetchTool.tool_params, "\"required\":[\"repository\",\"commit\"]") != null);
    try testing.expectEqualStrings("repo_fetch", RepoFetchTool.tool_name);
}
