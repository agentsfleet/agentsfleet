const std = @import("std");
const types = @import("types.zig");

const log = std.log.scoped(.scoring);

const MAX_STDERR_LINES: usize = 200;
const REDACTED = "[REDACTED]";
const NEVER_FLAG_KEYS = [_][]const u8{
    "ENCRYPTION_MASTER_KEY",
    "GITHUB_APP_ID",
    "GITHUB_APP_PRIVATE_KEY",
    "OIDC_PROVIDER",
    "OIDC_JWKS_URL",
    "OIDC_ISSUER",
    "OIDC_AUDIENCE",
    "API_KEY",
    "DATABASE_URL_API",
    "DATABASE_URL_WORKER",
    "REDIS_URL_API",
    "REDIS_URL_WORKER",
    "POSTHOG_API_KEY",
    "RESEND_API_KEY",
};

fn containsIgnoreCase(haystack: []const u8, needle: []const u8) bool {
    if (needle.len == 0) return true;
    if (haystack.len < needle.len) return false;

    var i: usize = 0;
    while (i + needle.len <= haystack.len) : (i += 1) {
        if (std.ascii.eqlIgnoreCase(haystack[i .. i + needle.len], needle)) return true;
    }
    return false;
}

pub fn classifyFailureFromErrorName(error_name: []const u8) ?types.FailureClass {
    if (std.mem.eql(u8, error_name, "RunDeadlineExceeded") or
        std.mem.eql(u8, error_name, "CommandTimedOut") or
        std.mem.eql(u8, error_name, "Timeout") or
        std.mem.eql(u8, error_name, "TimedOut"))
    {
        return .timeout;
    }

    if (std.mem.eql(u8, error_name, "OutOfMemory") or
        std.mem.eql(u8, error_name, "NoSpaceLeft"))
    {
        return .oom;
    }

    if (std.mem.eql(u8, error_name, "MissingConfig") or
        std.mem.eql(u8, error_name, "MissingGitHubInstallation") or
        std.mem.eql(u8, error_name, "MissingMasterKey") or
        std.mem.eql(u8, error_name, "PrAuthFailed") or
        std.mem.eql(u8, error_name, "AuthFailed") or
        std.mem.eql(u8, error_name, "TokenExpired") or
        std.mem.eql(u8, error_name, "Unauthorized") or
        std.mem.eql(u8, error_name, "RedisAuthFailed") or
        std.mem.eql(u8, error_name, "InvalidAuthorization"))
    {
        return .auth_failure;
    }

    if ((containsIgnoreCase(error_name, "context") and
        (containsIgnoreCase(error_name, "overflow") or
            containsIgnoreCase(error_name, "exhaust") or
            containsIgnoreCase(error_name, "window"))) or
        (containsIgnoreCase(error_name, "token") and
            (containsIgnoreCase(error_name, "limit") or
                containsIgnoreCase(error_name, "overflow") or
                containsIgnoreCase(error_name, "exceed"))))
    {
        return .context_overflow;
    }

    if (std.mem.eql(u8, error_name, "FileNotFound") or
        std.mem.eql(u8, error_name, "PathTraversal") or
        std.mem.eql(u8, error_name, "CommandFailed") or
        std.mem.eql(u8, error_name, "InvalidResponse"))
    {
        return .tool_call_failure;
    }

    return null;
}

pub fn classifyFailure(state: *const types.ScoringState) ?types.FailureClass {
    if (state.failure_class_override) |failure_class| {
        log.debug("classify override={s}", .{failure_class.label()});
        return failure_class;
    }
    if (state.failure_error_name) |error_name| {
        if (classifyFailureFromErrorName(error_name)) |failure_class| {
            log.debug("classify error_name={s} class={s}", .{ error_name, failure_class.label() });
            return failure_class;
        }
    }

    const result: ?types.FailureClass = switch (state.outcome) {
        .done => null,
        .blocked_retries_exhausted => .timeout,
        .blocked_gate_exhausted => .tool_call_failure,
        .blocked_stage_graph => .bad_output_format,
        .error_propagation => .unhandled_exception,
        .pending => .unknown,
        // M17_001 §1.2: cancelled by operator or resource limit — not an agent failure.
        .cancelled => null,
    };
    if (result) |fc| {
        log.debug("classify outcome_fallback class={s}", .{fc.label()});
    }
    return result;
}

pub fn scoreTierLabel(score: i32) []const u8 {
    if (score >= 90) return "Elite";
    if (score >= 70) return "Gold";
    if (score >= 40) return "Silver";
    return "Bronze";
}

pub fn scrubSecretAssignments(alloc: std.mem.Allocator, input: []const u8) ![]u8 {
    var out = std.ArrayList(u8){};
    errdefer out.deinit(alloc);

    var i: usize = 0;
    while (i < input.len) {
        var matched_key: ?[]const u8 = null;
        for (NEVER_FLAG_KEYS) |key| {
            if (i + key.len + 1 <= input.len and std.mem.eql(u8, input[i .. i + key.len], key) and input[i + key.len] == '=') {
                matched_key = key;
                break;
            }
        }

        if (matched_key) |key| {
            try out.appendSlice(alloc, key);
            try out.append(alloc, '=');
            try out.appendSlice(alloc, REDACTED);
            i += key.len + 1;
            while (i < input.len and input[i] != '\n' and input[i] != '\r' and input[i] != ' ' and input[i] != '\t' and input[i] != '"' and input[i] != '\'') : (i += 1) {}
            continue;
        }

        if (i + "Bearer ".len <= input.len and std.ascii.eqlIgnoreCase(input[i .. i + "Bearer ".len], "Bearer ")) {
            try out.appendSlice(alloc, "Bearer ");
            try out.appendSlice(alloc, REDACTED);
            i += "Bearer ".len;
            while (i < input.len and input[i] != '\n' and input[i] != '\r' and input[i] != ' ' and input[i] != '\t') : (i += 1) {}
            continue;
        }

        if (i + "-----BEGIN".len <= input.len and std.mem.eql(u8, input[i .. i + "-----BEGIN".len], "-----BEGIN")) {
            const end_idx = std.mem.indexOfPos(u8, input, i, "-----END");
            if (end_idx) |end_start| {
                const line_end = std.mem.indexOfScalarPos(u8, input, end_start, '\n') orelse input.len;
                try out.appendSlice(alloc, REDACTED);
                i = line_end;
                continue;
            }
        }

        try out.append(alloc, input[i]);
        i += 1;
    }

    return try out.toOwnedSlice(alloc);
}

fn lastLinesSlice(input: []const u8, max_lines: usize) []const u8 {
    if (input.len == 0) return input;
    var line_count: usize = 0;
    var idx: usize = input.len;
    while (idx > 0) {
        idx -= 1;
        if (input[idx] == '\n') {
            line_count += 1;
            if (line_count >= max_lines) return input[idx + 1 ..];
        }
    }
    return input;
}

pub fn scrubStderrTail(alloc: std.mem.Allocator, raw_tail: ?[]const u8) !?[]const u8 {
    const raw = raw_tail orelse return null;
    const trimmed = std.mem.trim(u8, raw, " \t\r\n");
    if (trimmed.len == 0) return null;
    const tail = lastLinesSlice(trimmed, MAX_STDERR_LINES);
    const scrubbed = try scrubSecretAssignments(alloc, tail);
    return scrubbed;
}

// T1 + T9 — classifyFailureFromErrorName: each known error name group routes to its class
test "classifyFailureFromErrorName routes known error names" {
    inline for (.{
        .{ "RunDeadlineExceeded", types.FailureClass.timeout },
        .{ "CommandTimedOut", types.FailureClass.timeout },
        .{ "Timeout", types.FailureClass.timeout },
        .{ "TimedOut", types.FailureClass.timeout },
        .{ "OutOfMemory", types.FailureClass.oom },
        .{ "NoSpaceLeft", types.FailureClass.oom },
        .{ "AuthFailed", types.FailureClass.auth_failure },
        .{ "TokenExpired", types.FailureClass.auth_failure },
        .{ "MissingConfig", types.FailureClass.auth_failure },
        .{ "FileNotFound", types.FailureClass.tool_call_failure },
        .{ "CommandFailed", types.FailureClass.tool_call_failure },
    }) |pair| {
        const result = classifyFailureFromErrorName(pair[0]);
        try std.testing.expect(result != null);
        try std.testing.expectEqual(pair[1], result.?);
    }
}

// T2 — empty string and unknown names return null
test "classifyFailureFromErrorName returns null for unknown names" {
    try std.testing.expectEqual(@as(?types.FailureClass, null), classifyFailureFromErrorName(""));
    try std.testing.expectEqual(@as(?types.FailureClass, null), classifyFailureFromErrorName("SomethingRandom"));
    try std.testing.expectEqual(@as(?types.FailureClass, null), classifyFailureFromErrorName("NetworkError"));
}

// T2 — context overflow matched case-insensitively via substring
test "classifyFailureFromErrorName detects context overflow variants" {
    try std.testing.expectEqual(types.FailureClass.context_overflow, classifyFailureFromErrorName("ContextWindowExhausted").?);
    try std.testing.expectEqual(types.FailureClass.context_overflow, classifyFailureFromErrorName("TokenLimitExceeded").?);
    try std.testing.expectEqual(types.FailureClass.context_overflow, classifyFailureFromErrorName("context_overflow").?);
}

// T1 + T9 — scoreTierLabel at all tier boundaries
test "scoreTierLabel returns correct tier at all boundaries" {
    inline for (.{
        .{ @as(i32, 0), "Bronze" },
        .{ @as(i32, 39), "Bronze" },
        .{ @as(i32, 40), "Silver" },
        .{ @as(i32, 69), "Silver" },
        .{ @as(i32, 70), "Gold" },
        .{ @as(i32, 89), "Gold" },
        .{ @as(i32, 90), "Elite" },
        .{ @as(i32, 100), "Elite" },
    }) |pair| {
        try std.testing.expectEqualStrings(pair[1], scoreTierLabel(pair[0]));
    }
}

// T1 + T8 — scrubSecretAssignments redacts NEVER_FLAG_KEY assignments
test "scrubSecretAssignments redacts known secret key assignments" {
    const alloc = std.testing.allocator;
    const input = "API_KEY=supersecret other=value";
    const out = try scrubSecretAssignments(alloc, input);
    defer alloc.free(out);
    try std.testing.expect(std.mem.indexOf(u8, out, "supersecret") == null);
    try std.testing.expect(std.mem.indexOf(u8, out, "API_KEY=[REDACTED]") != null);
}

// T8 — Bearer token redacted; T2 — innocuous text passes through unchanged
test "scrubSecretAssignments redacts Bearer tokens and passes innocuous text" {
    const alloc = std.testing.allocator;
    const bearer_input = "Authorization: Bearer ghp_xyz123";
    const bearer_out = try scrubSecretAssignments(alloc, bearer_input);
    defer alloc.free(bearer_out);
    try std.testing.expect(std.mem.indexOf(u8, bearer_out, "ghp_xyz123") == null);
    try std.testing.expect(std.mem.indexOf(u8, bearer_out, "[REDACTED]") != null);

    const safe_input = "exit code 0: task completed";
    const safe_out = try scrubSecretAssignments(alloc, safe_input);
    defer alloc.free(safe_out);
    try std.testing.expectEqualStrings(safe_input, safe_out);
}

// T2 — scrubStderrTail: null and whitespace-only inputs return null
test "scrubStderrTail returns null for null and whitespace-only inputs" {
    const alloc = std.testing.allocator;
    try std.testing.expectEqual(@as(?[]const u8, null), try scrubStderrTail(alloc, null));
    try std.testing.expectEqual(@as(?[]const u8, null), try scrubStderrTail(alloc, "   \n\t  "));
}

// T1 — classifyFailure uses failure_class_override when set
test "classifyFailure honours failure_class_override over error_name and outcome" {
    var state = types.ScoringState{};
    state.outcome = .done;
    state.failure_class_override = .oom;
    state.failure_error_name = "RunDeadlineExceeded";
    try std.testing.expectEqual(types.FailureClass.oom, classifyFailure(&state).?);
}

// T1 — classifyFailure falls through to outcome-based classification
test "classifyFailure maps outcome variants when no override or error_name" {
    inline for (.{
        .{ types.TerminalOutcome.done, @as(?types.FailureClass, null) },
        .{ types.TerminalOutcome.blocked_retries_exhausted, @as(?types.FailureClass, types.FailureClass.timeout) },
        .{ types.TerminalOutcome.blocked_stage_graph, @as(?types.FailureClass, types.FailureClass.bad_output_format) },
        .{ types.TerminalOutcome.error_propagation, @as(?types.FailureClass, types.FailureClass.unhandled_exception) },
        .{ types.TerminalOutcome.pending, @as(?types.FailureClass, types.FailureClass.unknown) },
    }) |pair| {
        const state = types.ScoringState{ .outcome = pair[0] };
        try std.testing.expectEqual(pair[1], classifyFailure(&state));
    }
}
