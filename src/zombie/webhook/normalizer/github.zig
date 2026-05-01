//! GitHub `workflow_run` webhook payload → flat M42 envelope JSON bytes.
//!
//! Extracts the ~10 fields the agent reasons over from the ~80-field GH
//! payload and emits a canonical flat object suitable for storing as the
//! envelope's `request_json`. Caller owns the returned slice.
//!
//! Filtering (action / event type / conclusion) is the handler's
//! responsibility. This module assumes the caller already decided to ingest.

const std = @import("std");

pub const NormalizeError = error{
    MalformedJson,
    MissingWorkflowRun,
    MissingRepository,
};

/// Flat envelope payload mirrored to JSON. Field names match the spec's
/// `request_json` contract; the agent reasons over these directly.
const Normalized = struct {
    run_url: []const u8,
    head_sha: []const u8,
    conclusion: []const u8,
    repo: []const u8,
    attempt: i64,
    run_id: i64,
    head_branch: []const u8,
    workflow_name: []const u8,
    received_at: []const u8,
};

/// Parse `raw_body` as a GitHub workflow_run webhook payload and emit the
/// canonical flat JSON object as owned bytes. `received_at_unix` is the
/// server-side receipt timestamp in seconds since epoch — emitted as RFC3339.
///
/// Callers that have already parsed the body (e.g. the handler that runs
/// the action filter inline) should use `normalizeFromValue` to avoid a
/// redundant parse + allocation on the happy path.
pub fn normalize(
    alloc: std.mem.Allocator,
    raw_body: []const u8,
    received_at_unix: i64,
) ![]u8 {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, raw_body, .{}) catch return NormalizeError.MalformedJson;
    defer parsed.deinit();
    const root = switch (parsed.value) {
        .object => |o| o,
        else => return NormalizeError.MalformedJson,
    };
    return normalizeFromValue(alloc, root, received_at_unix);
}

/// Same as `normalize`, but the caller owns the parsed root. Borrowed string
/// slices in `root` must outlive this call (they're copied by `valueAlloc`
/// before the allocation returns).
pub fn normalizeFromValue(
    alloc: std.mem.Allocator,
    root: std.json.ObjectMap,
    received_at_unix: i64,
) ![]u8 {
    const wr_val = root.get("workflow_run") orelse return NormalizeError.MissingWorkflowRun;
    const wr = switch (wr_val) {
        .object => |o| o,
        else => return NormalizeError.MissingWorkflowRun,
    };
    const repo_val = root.get("repository") orelse return NormalizeError.MissingRepository;
    const repo = switch (repo_val) {
        .object => |o| o,
        else => return NormalizeError.MissingRepository,
    };

    var ts_buf: [32]u8 = undefined;
    const out = Normalized{
        .run_url = jsonString(wr.get("html_url")) orelse "",
        .head_sha = jsonString(wr.get("head_sha")) orelse "",
        .conclusion = jsonString(wr.get("conclusion")) orelse "",
        .repo = jsonString(repo.get("full_name")) orelse "",
        .attempt = jsonNumberAsI64(wr.get("run_attempt")) orelse 1,
        .run_id = jsonNumberAsI64(wr.get("id")) orelse 0,
        .head_branch = jsonString(wr.get("head_branch")) orelse "",
        .workflow_name = jsonString(wr.get("name")) orelse "",
        .received_at = formatRfc3339(&ts_buf, received_at_unix),
    };
    return std.json.Stringify.valueAlloc(alloc, out, .{});
}

fn jsonString(v: ?std.json.Value) ?[]const u8 {
    const val = v orelse return null;
    return switch (val) {
        .string => |s| s,
        else => null,
    };
}

fn jsonNumberAsI64(v: ?std.json.Value) ?i64 {
    const val = v orelse return null;
    return switch (val) {
        .integer => |i| i,
        .float => |f| @intFromFloat(f),
        .number_string => |s| std.fmt.parseInt(i64, s, 10) catch null,
        else => null,
    };
}

fn formatRfc3339(buf: []u8, unix_seconds: i64) []const u8 {
    const epoch_secs: u64 = if (unix_seconds < 0) 0 else @intCast(unix_seconds);
    const epoch = std.time.epoch.EpochSeconds{ .secs = epoch_secs };
    const day = epoch.getEpochDay();
    const ymd = day.calculateYearDay();
    const md = ymd.calculateMonthDay();
    const ds = epoch.getDaySeconds();
    return std.fmt.bufPrint(buf, "{d:0>4}-{d:0>2}-{d:0>2}T{d:0>2}:{d:0>2}:{d:0>2}Z", .{
        ymd.year,
        @intFromEnum(md.month),
        md.day_index + 1,
        ds.getHoursIntoDay(),
        ds.getMinutesIntoHour(),
        ds.getSecondsIntoMinute(),
    }) catch unreachable;
}

test {
    _ = @import("github_test.zig");
}
