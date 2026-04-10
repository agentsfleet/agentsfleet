//! Response body content scanner for the AI Firewall.
//!
//! Inspects response bodies from external APIs before returning to the agent.
//! Two scan types:
//!   1. Credential leakage — check if vault credential values appear in response.
//!   2. PII / API key detection — flag credit cards, SSNs, API keys.
//!
//! Detected content is flagged (not blocked). The response proceeds with a
//! warning in the activity stream.

const std = @import("std");

pub const MAX_SCAN_SIZE: usize = 1_048_576; // 1MB

pub const FlagType = enum {
    pii_credit_card,
    pii_ssn,
    api_key_leak,
    credential_echo,
};

pub const ScanResult = union(enum) {
    clean: void,
    flagged: struct {
        flag_type: FlagType,
        detail: []const u8,
    },
    truncated: struct {
        scanned_bytes: usize,
        total_bytes: usize,
    },
};

/// Scan a response body for sensitive content.
/// If `body.len > MAX_SCAN_SIZE`, scans only the first 1MB and returns `.truncated`.
/// `credentials` contains vault credential values to check for echo.
pub fn scanResponse(body: []const u8, credentials: []const []const u8) ScanResult {
    const scan_len = @min(body.len, MAX_SCAN_SIZE);
    const scan_body = body[0..scan_len];

    // 1. Credential echo check
    for (credentials) |cred| {
        if (cred.len >= 4 and std.mem.indexOf(u8, scan_body, cred) != null) {
            return .{ .flagged = .{
                .flag_type = .credential_echo,
                .detail = "Vault credential value found in response body",
            } };
        }
    }

    // 2. API key patterns (check before PII — more specific)
    if (checkApiKeyPatterns(scan_body)) |detail| {
        return .{ .flagged = .{ .flag_type = .api_key_leak, .detail = detail } };
    }

    // 3. Credit card patterns
    if (checkCreditCardPattern(scan_body)) {
        return .{ .flagged = .{
            .flag_type = .pii_credit_card,
            .detail = "Credit card pattern detected in response",
        } };
    }

    // 4. SSN pattern
    if (checkSsnPattern(scan_body)) {
        return .{ .flagged = .{
            .flag_type = .pii_ssn,
            .detail = "SSN pattern detected in response",
        } };
    }

    if (body.len > MAX_SCAN_SIZE) {
        return .{ .truncated = .{
            .scanned_bytes = scan_len,
            .total_bytes = body.len,
        } };
    }

    return .{ .clean = {} };
}

fn checkApiKeyPatterns(body: []const u8) ?[]const u8 {
    const prefixes = [_]struct { prefix: []const u8, label: []const u8 }{
        .{ .prefix = "sk-proj-", .label = "OpenAI API key pattern in response" },
        .{ .prefix = "sk-ant-", .label = "Anthropic API key pattern in response" },
        .{ .prefix = "ghp_", .label = "GitHub personal access token in response" },
        .{ .prefix = "ghs_", .label = "GitHub server token in response" },
        .{ .prefix = "xoxb-", .label = "Slack bot token in response" },
        .{ .prefix = "xoxp-", .label = "Slack user token in response" },
        .{ .prefix = "sk_live_", .label = "Stripe live secret key in response" },
        .{ .prefix = "rk_live_", .label = "Stripe restricted key in response" },
    };
    for (&prefixes) |entry| {
        if (std.mem.indexOf(u8, body, entry.prefix) != null) return entry.label;
    }
    return null;
}

/// Detect credit card number patterns: 4 groups of 4 digits separated by spaces or dashes,
/// or 16 consecutive digits. Validates with Luhn checksum to reduce false positives.
fn checkCreditCardPattern(body: []const u8) bool {
    var i: usize = 0;
    while (i + 18 < body.len) : (i += 1) {
        if (isDigit(body[i]) and matchCardPattern(body[i..])) {
            const digits = extractDigits(body[i .. i + 19]);
            if (luhnCheck(&digits)) return true;
        }
        if (i + 15 < body.len and isDigit(body[i]) and allDigits(body[i .. i + 16])) {
            if (luhnCheck(body[i .. i + 16])) return true;
        }
    }
    while (i + 15 < body.len) : (i += 1) {
        if (isDigit(body[i]) and allDigits(body[i .. i + 16])) {
            if (luhnCheck(body[i .. i + 16])) return true;
        }
    }
    return false;
}

/// Test-only accessor for extractDigits.
pub const extractDigitsForTest = extractDigits;

/// Extract only digit characters from a card-formatted string (skip spaces/dashes).
fn extractDigits(s: []const u8) [16]u8 {
    var digits: [16]u8 = .{0} ** 16;
    var count: usize = 0;
    for (s) |c| {
        if (isDigit(c) and count < 16) {
            digits[count] = c;
            count += 1;
        }
    }
    return digits;
}

/// Luhn checksum validation. Returns true if the digit sequence passes.
fn luhnCheck(digits: []const u8) bool {
    if (digits.len < 13 or digits.len > 19) return false;
    var sum: u32 = 0;
    var alt = false;
    var idx = digits.len;
    while (idx > 0) {
        idx -= 1;
        if (!isDigit(digits[idx])) return false;
        var n: u32 = digits[idx] - '0';
        if (alt) {
            n *= 2;
            if (n > 9) n -= 9;
        }
        sum += n;
        alt = !alt;
    }
    return sum % 10 == 0;
}

fn matchCardPattern(s: []const u8) bool {
    if (s.len < 19) return false;
    // DDDD[- ]DDDD[- ]DDDD[- ]DDDD
    var pos: usize = 0;
    for (0..4) |group| {
        for (0..4) |_| {
            if (pos >= s.len or !isDigit(s[pos])) return false;
            pos += 1;
        }
        if (group < 3) {
            if (pos >= s.len or (s[pos] != ' ' and s[pos] != '-')) return false;
            pos += 1;
        }
    }
    return true;
}

fn allDigits(s: []const u8) bool {
    for (s) |c| {
        if (!isDigit(c)) return false;
    }
    return true;
}

fn isDigit(c: u8) bool {
    return c >= '0' and c <= '9';
}

/// Detect SSN pattern: 3 digits, dash, 2 digits, dash, 4 digits (XXX-XX-XXXX).
fn checkSsnPattern(body: []const u8) bool {
    if (body.len < 11) return false;
    var i: usize = 0;
    while (i + 10 < body.len) : (i += 1) {
        if (isDigit(body[i]) and isDigit(body[i + 1]) and isDigit(body[i + 2]) and
            body[i + 3] == '-' and
            isDigit(body[i + 4]) and isDigit(body[i + 5]) and
            body[i + 6] == '-' and
            isDigit(body[i + 7]) and isDigit(body[i + 8]) and isDigit(body[i + 9]) and isDigit(body[i + 10]))
        {
            return true;
        }
    }
    return false;
}

// ── Tests ──────────────────────────────────────────────────────────────────

test "4.1: detects credit card pattern" {
    const result = scanResponse("Payment info: 4111 1111 1111 1111 for order", &.{});
    switch (result) {
        .flagged => |f| {
            try std.testing.expect(f.flag_type == .pii_credit_card);
            try std.testing.expect(std.mem.indexOf(u8, f.detail, "Credit card") != null);
        },
        else => return error.ExpectedFlagged,
    }
}

test "4.2: detects OpenAI API key pattern" {
    const result = scanResponse("Here is your key: sk-proj-abc123def456", &.{});
    switch (result) {
        .flagged => |f| {
            try std.testing.expect(f.flag_type == .api_key_leak);
            try std.testing.expect(std.mem.indexOf(u8, f.detail, "OpenAI") != null);
        },
        else => return error.ExpectedFlagged,
    }
}

test "4.3: normal JSON response is clean" {
    const result = scanResponse("{\"id\": \"ch_123\", \"amount\": 4700}", &.{});
    try std.testing.expect(result == .clean);
}

test "4.4: large response scans first 1MB only" {
    const alloc = std.testing.allocator;
    // Create a body > 1MB of clean data
    const big = try alloc.alloc(u8, MAX_SCAN_SIZE + 1024);
    defer alloc.free(big);
    @memset(big, 'a');

    const result = scanResponse(big, &.{});
    switch (result) {
        .truncated => |t| {
            try std.testing.expectEqual(MAX_SCAN_SIZE, t.scanned_bytes);
            try std.testing.expectEqual(MAX_SCAN_SIZE + 1024, t.total_bytes);
        },
        else => return error.ExpectedTruncated,
    }
}

test "detects credential echo" {
    const creds = &[_][]const u8{"super-secret-token-xyz"};
    const result = scanResponse("Response contains super-secret-token-xyz in body", creds);
    switch (result) {
        .flagged => |f| try std.testing.expect(f.flag_type == .credential_echo),
        else => return error.ExpectedFlagged,
    }
}

test "short credentials (< 4 chars) are not checked" {
    const creds = &[_][]const u8{"ab"};
    const result = scanResponse("ab is in body", creds);
    try std.testing.expect(result == .clean);
}

test "detects SSN pattern" {
    const result = scanResponse("SSN: 123-45-6789 on file", &.{});
    switch (result) {
        .flagged => |f| try std.testing.expect(f.flag_type == .pii_ssn),
        else => return error.ExpectedFlagged,
    }
}

test "detects Stripe live key" {
    const result = scanResponse("key=sk_live_abc123", &.{});
    switch (result) {
        .flagged => |f| try std.testing.expect(f.flag_type == .api_key_leak),
        else => return error.ExpectedFlagged,
    }
}

test "detects credit card with dashes" {
    const result = scanResponse("Card: 4111-1111-1111-1111", &.{});
    switch (result) {
        .flagged => |f| try std.testing.expect(f.flag_type == .pii_credit_card),
        else => return error.ExpectedFlagged,
    }
}
