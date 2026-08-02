//! Does the approved diff stay inside the files its proposal declared?
//!
//! The apply path re-asks this question against the approved bytes, after the
//! human has already said yes. A proposal's file list is the promise a reviewer
//! actually approved; the diff is what would be written. Nothing guarantees the
//! two agree unless something checks, and that is this module.
//!
//! The path extraction is deliberately over-eager: any line that could be read
//! as a file header by the tool applying the patch is treated as one, even
//! where a strict unified-diff parser would call it hunk content. The asymmetry
//! is the point. Over-detection costs a false refusal, which an operator sees
//! with a named path and a stable code; under-detection would let a header
//! smuggled inside a hunk write a file nobody approved. One of those is
//! recoverable. A diff whose content lines themselves begin with "++ " or
//! "-- " therefore refuses conservatively — rare in a code fix, and a refusal
//! rather than a surprise write.

const std = @import("std");
const repair_proposal = @import("repair_proposal.zig");

const HEADER_OLD = "--- ";
const HEADER_NEW = "+++ ";
const HEADER_GIT = "diff --git ";
const DEV_NULL = "/dev/null";
const OLD_PATH_PREFIX = "a/";
const NEW_PATH_PREFIX = "b/";
/// Separates the two paths of a `diff --git a/x b/y` header.
const GIT_PAIR_SEPARATOR = " " ++ NEW_PATH_PREFIX;
const LINE_SEPARATOR = '\n';
const CARRIAGE_RETURN = '\r';
/// Unified-diff headers may carry a tab-separated timestamp after the path.
const TIMESTAMP_SEPARATOR = '\t';
const MAX_PATHS_PER_HEADER = 2;

comptime {
    // Both markers are sliced past one fixed width below.
    std.debug.assert(HEADER_OLD.len == HEADER_NEW.len);
}
const HEADER_MARKER_LEN = HEADER_OLD.len;

pub const Violation = union(enum) {
    /// A path the diff would touch that the proposal never declared. Borrowed
    /// from the diff bytes, so it lives exactly as long as the proposal does.
    path_outside_allowlist: []const u8,
    too_many_files: usize,
    diff_too_large: usize,
};

pub const Result = union(enum) {
    ok,
    violated: Violation,

    /// Every bounds violation refuses under one code — the detail above is for
    /// the operator, the code is for the activity stream and the Slack notice.
    pub fn refusal(self: Result) ?repair_proposal.Refusal {
        return switch (self) {
            .ok => null,
            .violated => .bounds_exceeded,
        };
    }
};

/// Re-check the approved bytes at apply time.
pub fn check(p: repair_proposal.Proposal) Result {
    if (p.files.len > repair_proposal.MAX_FILES) {
        return .{ .violated = .{ .too_many_files = p.files.len } };
    }
    if (p.diff.len > repair_proposal.MAX_DIFF_BYTES) {
        return .{ .violated = .{ .diff_too_large = p.diff.len } };
    }
    var lines = std.mem.splitScalar(u8, p.diff, LINE_SEPARATOR);
    while (lines.next()) |line| {
        var paths: [MAX_PATHS_PER_HEADER][]const u8 = undefined;
        const found = headerPaths(line, &paths);
        for (paths[0..found]) |path| {
            if (!isAllowed(p.files, path)) {
                return .{ .violated = .{ .path_outside_allowlist = path } };
            }
        }
    }
    return .ok;
}

/// Collect every path a line could name as a file header. Returns how many of
/// `out` were filled; zero means the line names no path.
fn headerPaths(line: []const u8, out: *[MAX_PATHS_PER_HEADER][]const u8) usize {
    if (std.mem.startsWith(u8, line, HEADER_GIT)) {
        const rest = trimDecoration(line[HEADER_GIT.len..]);
        // A rename names two different paths, and both must be allowed. Split
        // on the LAST " b/" so a left-hand path containing spaces still cuts
        // in the right place.
        const sep = std.mem.lastIndexOf(u8, rest, GIT_PAIR_SEPARATOR) orelse {
            out[0] = stripPathPrefix(rest);
            return 1;
        };
        out[0] = stripPathPrefix(rest[0..sep]);
        out[1] = stripPathPrefix(rest[sep + 1 ..]);
        return 2;
    }
    if (std.mem.startsWith(u8, line, HEADER_OLD) or std.mem.startsWith(u8, line, HEADER_NEW)) {
        const path = trimDecoration(line[HEADER_MARKER_LEN..]);
        // A created or deleted file names /dev/null on one side; only the real
        // side has to be in the allowlist.
        if (std.mem.eql(u8, path, DEV_NULL)) return 0;
        out[0] = stripPathPrefix(path);
        return 1;
    }
    return 0;
}

fn stripPathPrefix(path: []const u8) []const u8 {
    if (std.mem.startsWith(u8, path, OLD_PATH_PREFIX)) return path[OLD_PATH_PREFIX.len..];
    if (std.mem.startsWith(u8, path, NEW_PATH_PREFIX)) return path[NEW_PATH_PREFIX.len..];
    return path;
}

fn trimDecoration(raw: []const u8) []const u8 {
    const no_timestamp = if (std.mem.indexOfScalar(u8, raw, TIMESTAMP_SEPARATOR)) |i| raw[0..i] else raw;
    return std.mem.trimEnd(u8, no_timestamp, &.{CARRIAGE_RETURN});
}

fn isAllowed(allowlist: []const []const u8, path: []const u8) bool {
    for (allowlist) |allowed| {
        if (std.mem.eql(u8, allowed, path)) return true;
    }
    return false;
}

test {
    _ = @import("repair_bounds_test.zig");
}
