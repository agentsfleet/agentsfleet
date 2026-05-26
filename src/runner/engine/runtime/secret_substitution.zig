//! `${secrets.NAME.FIELD}` → resolved-value substitution scanner.
//!
//! The agent emits tool-call args carrying placeholder strings (e.g.
//! `Authorization: Bearer ${secrets.fly.api_token}`). Before the outbound
//! HTTPS request fires — and AFTER the sandbox boundary has closed
//! (Landlock + cgroups + bwrap) — we walk the args, look up each
//! placeholder's value in the session's `secrets_map`, and rewrite into
//! a fresh buffer. The rewritten bytes are what hit the wire; the
//! placeholder bytes are what the agent's frame log sees (existing
//! redaction path in `runner_progress.Adapter`).
//!
//! Safety contract:
//!   - Empty `secrets_map` + a placeholder is `MissingSecret` (fail
//!     closed; agent sees the error and reformulates).
//!   - Non-string field value is `NotAString` (the secrets_map field
//!     traversal lands on something we can't substitute as bytes).
//!   - After substitution, the output MUST NOT contain `${secrets.`
//!     anywhere — partial substitution is a leak vector. Caller can
//!     enforce via `assertNoLeftover`.
//!
//! Placeholder grammar (intentionally narrow — keeps the scanner
//! tractable and rejects ambiguous inputs):
//!     ${secrets.<name>.<field>}
//!     name, field: [A-Za-z_][A-Za-z0-9_]*

const std = @import("std");

pub const SubstitutionError = error{
    /// Placeholder syntax is malformed (unterminated, unexpected char).
    MalformedPlaceholder,
    /// `secrets_map[name]` not present.
    MissingSecret,
    /// `secrets_map[name].field` not present.
    MissingField,
    /// Field value isn't a JSON string — can't substitute as bytes.
    NotAString,
};

const placeholder_prefix: []const u8 = "${secrets.";
const placeholder_suffix: u8 = '}';

/// Walk `raw` and produce a fresh buffer with every `${secrets.NAME.FIELD}`
/// replaced by `secrets_map[NAME][FIELD]`. Caller owns the returned slice.
///
/// `secrets_map` must be a `.object` JSON value whose keys are credential
/// names and whose values are `.object`s holding `.string` fields. Any
/// other shape produces a typed error. An empty placeholder set returns a
/// straight dupe of `raw` (caller frees in either case — uniform ownership
/// beats a borrow-or-own union for one allocation per call).
pub fn substitute(
    alloc: std.mem.Allocator,
    raw: []const u8,
    secrets_map: ?std.json.Value,
) SubstitutionError![]u8 {
    var out: std.ArrayList(u8) = .{};
    out.ensureTotalCapacity(alloc, raw.len) catch return error.MissingSecret;
    errdefer out.deinit(alloc);

    var i: usize = 0;
    while (i < raw.len) {
        if (i + placeholder_prefix.len <= raw.len and
            std.mem.eql(u8, raw[i .. i + placeholder_prefix.len], placeholder_prefix))
        {
            const after_prefix = i + placeholder_prefix.len;
            const close = std.mem.indexOfScalarPos(u8, raw, after_prefix, placeholder_suffix) orelse return error.MalformedPlaceholder;
            const inner = raw[after_prefix..close];

            const dot = std.mem.indexOfScalar(u8, inner, '.') orelse return error.MalformedPlaceholder;
            const name = inner[0..dot];
            const field = inner[dot + 1 ..];
            if (!isIdentifier(name) or !isIdentifier(field)) return error.MalformedPlaceholder;

            const value = try lookupString(secrets_map, name, field);
            out.appendSlice(alloc, value) catch return error.MissingSecret;
            i = close + 1;
            continue;
        }
        out.append(alloc, raw[i]) catch return error.MissingSecret;
        i += 1;
    }

    return out.toOwnedSlice(alloc) catch error.MissingSecret;
}

/// Returns true when `out` contains no leftover `${secrets.` substring.
/// Call after `substitute` as a defence-in-depth check before the HTTP
/// fetch fires; refuse to send if the assert fails.
pub fn assertNoLeftover(out: []const u8) bool {
    return std.mem.indexOf(u8, out, placeholder_prefix) == null;
}

/// Identifier grammar: leading `[A-Za-z_]`, then `[A-Za-z0-9_]*`. Empty
/// string fails. Used for both name and field segments.
fn isIdentifier(s: []const u8) bool {
    if (s.len == 0) return false;
    const c0 = s[0];
    if (!((c0 >= 'A' and c0 <= 'Z') or (c0 >= 'a' and c0 <= 'z') or c0 == '_')) return false;
    for (s[1..]) |c| {
        const ok = (c >= 'A' and c <= 'Z') or (c >= 'a' and c <= 'z') or (c >= '0' and c <= '9') or c == '_';
        if (!ok) return false;
    }
    return true;
}

fn lookupString(secrets_map: ?std.json.Value, name: []const u8, field: []const u8) SubstitutionError![]const u8 {
    const sm = secrets_map orelse return error.MissingSecret;
    if (sm != .object) return error.MissingSecret;
    const cred = sm.object.get(name) orelse return error.MissingSecret;
    if (cred != .object) return error.MissingField;
    const f = cred.object.get(field) orelse return error.MissingField;
    return switch (f) {
        .string => |s| s,
        else => error.NotAString,
    };
}

// ── Tests ───────────────────────────────────────────────────────────────────

fn buildSecrets(arena: std.mem.Allocator) !std.json.Value {
    var fly = std.json.ObjectMap.init(arena);
    try fly.put("api_token", .{ .string = "FlyTokenXyz" });

    var slack = std.json.ObjectMap.init(arena);
    try slack.put("bot_token", .{ .string = "xoxb-AAA" });

    var top = std.json.ObjectMap.init(arena);
    try top.put("fly", .{ .object = fly });
    try top.put("slack", .{ .object = slack });
    return .{ .object = top };
}

test "substitute replaces a single placeholder" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const sm = try buildSecrets(arena);
    const out = try substitute(std.testing.allocator,
        "Authorization: Bearer ${secrets.fly.api_token}",
        sm);
    defer std.testing.allocator.free(out);
    try std.testing.expectEqualStrings("Authorization: Bearer FlyTokenXyz", out);
    try std.testing.expect(assertNoLeftover(out));
}

test "substitute handles multiple placeholders in one pass" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const sm = try buildSecrets(arena);
    const out = try substitute(std.testing.allocator,
        "fly=${secrets.fly.api_token},slack=${secrets.slack.bot_token}",
        sm);
    defer std.testing.allocator.free(out);
    try std.testing.expectEqualStrings("fly=FlyTokenXyz,slack=xoxb-AAA", out);
}

test "substitute leaves non-placeholder text untouched" {
    const out = try substitute(std.testing.allocator, "no secrets here", null);
    defer std.testing.allocator.free(out);
    try std.testing.expectEqualStrings("no secrets here", out);
    try std.testing.expect(assertNoLeftover(out));
}

test "substitute fails closed when secrets_map is null" {
    try std.testing.expectError(error.MissingSecret,
        substitute(std.testing.allocator, "${secrets.fly.api_token}", null));
}

test "substitute fails closed on missing credential name" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const sm = try buildSecrets(arena_state.allocator());
    try std.testing.expectError(error.MissingSecret,
        substitute(std.testing.allocator, "${secrets.unknown.x}", sm));
}

test "substitute fails closed on missing field" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const sm = try buildSecrets(arena_state.allocator());
    try std.testing.expectError(error.MissingField,
        substitute(std.testing.allocator, "${secrets.fly.unknown_field}", sm));
}

test "substitute fails closed on non-string field" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    var fly = std.json.ObjectMap.init(arena);
    try fly.put("api_token", .{ .integer = 42 });
    var top = std.json.ObjectMap.init(arena);
    try top.put("fly", .{ .object = fly });
    const sm: std.json.Value = .{ .object = top };

    try std.testing.expectError(error.NotAString,
        substitute(std.testing.allocator, "${secrets.fly.api_token}", sm));
}

test "substitute rejects malformed placeholder (no field separator)" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const sm = try buildSecrets(arena_state.allocator());
    try std.testing.expectError(error.MalformedPlaceholder,
        substitute(std.testing.allocator, "${secrets.fly}", sm));
}

test "substitute rejects unterminated placeholder" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const sm = try buildSecrets(arena_state.allocator());
    try std.testing.expectError(error.MalformedPlaceholder,
        substitute(std.testing.allocator, "${secrets.fly.api_token nope", sm));
}

test "substitute rejects identifier with hyphen" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const sm = try buildSecrets(arena_state.allocator());
    try std.testing.expectError(error.MalformedPlaceholder,
        substitute(std.testing.allocator, "${secrets.fly-prod.api_token}", sm));
}

test "assertNoLeftover catches partial substitution" {
    try std.testing.expect(!assertNoLeftover("real bytes ${secrets.x.y}"));
    try std.testing.expect(assertNoLeftover("real bytes only"));
    try std.testing.expect(assertNoLeftover(""));
}

test "substitute produces output safe for the no-leftover assert" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const sm = try buildSecrets(arena_state.allocator());

    const out = try substitute(std.testing.allocator,
        "${secrets.fly.api_token} and ${secrets.slack.bot_token}",
        sm);
    defer std.testing.allocator.free(out);
    try std.testing.expect(assertNoLeftover(out));
}
