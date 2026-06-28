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
const credential_request = @import("../credential_request.zig");

pub const SubstitutionError = error{
    /// Placeholder syntax is malformed (unterminated, unexpected char).
    MalformedPlaceholder,
    /// `secrets_map[name]` not present.
    MissingSecret,
    /// `secrets_map[name].field` not present.
    MissingField,
    /// Field value isn't a JSON string — can't substitute as bytes.
    NotAString,
    /// A mintable integration handle reached substitution but no mint channel
    /// was wired (config error) — fail closed rather than dispatch tokenless.
    MintUnavailable,
    /// The on-demand mint round-trip failed (transport loss, deadline, typed
    /// rejection, OOM). Collapsed to one cause; the channel logs the detail.
    MintFailed,
};

/// The only field a mintable integration handle answers: `${secrets.<id>.token}`.
/// A mint handle carries no static fields, so any other field fails closed.
const MINT_TOKEN_FIELD: []const u8 = "token";

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
    /// Per-call mint resolver, or null when no session wired one (the register-only
    /// / test path). A `${secrets.<name>.token}` whose name is in the resolver's
    /// mintable list mints via the channel; every other placeholder is a static
    /// `secrets_map` lookup. A mintable name with no wired channel fails closed.
    resolver: ?*credential_request.MintResolver,
) SubstitutionError![]u8 {
    var out: std.ArrayList(u8) = .empty;
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

            try appendResolved(alloc, &out, secrets_map, resolver, name, field);
            i = close + 1;
            continue;
        }
        out.append(alloc, raw[i]) catch return error.MissingSecret;
        i += 1;
    }

    return out.toOwnedSlice(alloc) catch error.MissingSecret;
}

/// Resolve one `${secrets.name.field}` into `out`. A mintable credential (the
/// resolver's `mintable` list names it) mints the token via the channel at dispatch
/// — deduped per call by the resolver; a static credential does the `secrets_map`
/// field lookup. The minted token is arena-owned by the resolver's per-call cache
/// (freed at call end) — we copy it into `out`, never alias it into the policy.
fn appendResolved(
    alloc: std.mem.Allocator,
    out: *std.ArrayList(u8),
    secrets_map: ?std.json.Value,
    resolver: ?*credential_request.MintResolver,
    name: []const u8,
    field: []const u8,
) SubstitutionError!void {
    if (resolver) |r| {
        if (r.integrationFor(name)) |integration_id| {
            // A mintable credential answers only `.token`; any other field is a
            // config error (the handle has no static fields to look up).
            if (!std.mem.eql(u8, field, MINT_TOKEN_FIELD)) return error.MissingField;
            const token = r.token(alloc, name, integration_id) catch |err| switch (err) {
                error.MintUnavailable => return error.MintUnavailable,
                error.OutOfMemory => return error.MissingSecret,
                error.MintFailed => return error.MintFailed,
            };
            out.appendSlice(alloc, token) catch return error.MissingSecret;
            return;
        }
    }
    const value = try lookupString(secrets_map, name, field);
    out.appendSlice(alloc, value) catch return error.MissingSecret;
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
    var fly: std.json.ObjectMap = .empty;
    try fly.put(arena, "api_token", .{ .string = "FlyTokenXyz" });

    var slack: std.json.ObjectMap = .empty;
    try slack.put(arena, "bot_token", .{ .string = "xoxb-AAA" });

    var top: std.json.ObjectMap = .empty;
    try top.put(arena, "fly", .{ .object = fly });
    try top.put(arena, "slack", .{ .object = slack });
    return .{ .object = top };
}

test "substitute replaces a single placeholder" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const sm = try buildSecrets(arena);
    const out = try substitute(std.testing.allocator, "Authorization: Bearer ${secrets.fly.api_token}", sm, null);
    defer std.testing.allocator.free(out);
    try std.testing.expectEqualStrings("Authorization: Bearer FlyTokenXyz", out);
    try std.testing.expect(assertNoLeftover(out));
}

test "substitute handles multiple placeholders in one pass" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const sm = try buildSecrets(arena);
    const out = try substitute(std.testing.allocator, "fly=${secrets.fly.api_token},slack=${secrets.slack.bot_token}", sm, null);
    defer std.testing.allocator.free(out);
    try std.testing.expectEqualStrings("fly=FlyTokenXyz,slack=xoxb-AAA", out);
}

test "substitute leaves non-placeholder text untouched" {
    const out = try substitute(std.testing.allocator, "no secrets here", null, null);
    defer std.testing.allocator.free(out);
    try std.testing.expectEqualStrings("no secrets here", out);
    try std.testing.expect(assertNoLeftover(out));
}

test "substitute fails closed when secrets_map is null" {
    try std.testing.expectError(error.MissingSecret, substitute(std.testing.allocator, "${secrets.fly.api_token}", null, null));
}

test "substitute fails closed on missing credential name" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const sm = try buildSecrets(arena_state.allocator());
    try std.testing.expectError(error.MissingSecret, substitute(std.testing.allocator, "${secrets.unknown.x}", sm, null));
}

test "substitute fails closed on missing field" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const sm = try buildSecrets(arena_state.allocator());
    try std.testing.expectError(error.MissingField, substitute(std.testing.allocator, "${secrets.fly.unknown_field}", sm, null));
}

test "substitute fails closed on non-string field" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    var fly: std.json.ObjectMap = .empty;
    try fly.put(arena, "api_token", .{ .integer = 42 });
    var top: std.json.ObjectMap = .empty;
    try top.put(arena, "fly", .{ .object = fly });
    const sm: std.json.Value = .{ .object = top };

    try std.testing.expectError(error.NotAString, substitute(std.testing.allocator, "${secrets.fly.api_token}", sm, null));
}

test "substitute rejects malformed placeholder (no field separator)" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const sm = try buildSecrets(arena_state.allocator());
    try std.testing.expectError(error.MalformedPlaceholder, substitute(std.testing.allocator, "${secrets.fly}", sm, null));
}

test "substitute rejects unterminated placeholder" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const sm = try buildSecrets(arena_state.allocator());
    try std.testing.expectError(error.MalformedPlaceholder, substitute(std.testing.allocator, "${secrets.fly.api_token nope", sm, null));
}

test "substitute rejects identifier with hyphen" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const sm = try buildSecrets(arena_state.allocator());
    try std.testing.expectError(error.MalformedPlaceholder, substitute(std.testing.allocator, "${secrets.fly-prod.api_token}", sm, null));
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

    const out = try substitute(std.testing.allocator, "${secrets.fly.api_token} and ${secrets.slack.bot_token}", sm, null);
    defer std.testing.allocator.free(out);
    try std.testing.expect(assertNoLeftover(out));
}

// ── Mint-path tests (M102 §4) ────────────────────────────────────────────────
// Drive substitution through a real `MintResolver` over OS pipes (the test plays
// the parent: it pre-buffers the minted-token reply the child reads).

const pipe_proto = @import("../../pipe_proto.zig");

/// A channel over a fresh pipe pair with the parent's response reply pre-buffered;
/// returns the channel + the four fds the caller closes. `req[1]`/`resp[0]` are the
/// child's ends (the Channel); `resp[1]` is where we framed the reply.
fn mintChannelWithReply(token_value: []const u8) !struct { ch: credential_request.Channel, fds: [4]std.posix.fd_t } {
    const clock = @import("common").clock;
    const req = try pipe_proto.testOsPipe(); // [read, write]; child writes req[1]
    const resp = try pipe_proto.testOsPipe(); // child reads resp[0]
    const reply = try std.json.Stringify.valueAlloc(std.testing.allocator, credential_request.PipeResponse{ .ok = true, .token = token_value, .expires_at_ms = 999 }, .{});
    defer std.testing.allocator.free(reply);
    try pipe_proto.writeFrame(resp[1], .credential_response, reply);
    return .{
        .ch = .{ .request_fd = req[1], .response_fd = resp[0], .deadline_ms = clock.nowMillis() + 5_000 },
        .fds = .{ req[0], req[1], resp[0], resp[1] },
    };
}

test "test_bridge_mints_on_placeholder" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const h = try mintChannelWithReply("ghs_minted");
    defer for (h.fds) |fd| pipe_proto.testOsClose(fd);

    // No static secrets_map — the github name resolves purely through the resolver.
    var resolver = credential_request.MintResolver{
        .mintable = &.{.{ .name = "github", .integration = "github" }},
        .channel = h.ch,
    };
    const out = try substitute(arena, "Authorization: Bearer ${secrets.github.token}", null, &resolver);
    try std.testing.expectEqualStrings("Authorization: Bearer ghs_minted", out);
    try std.testing.expect(assertNoLeftover(out)); // value substituted only at dispatch
}

test "substitute routes static vs mintable by the resolver's grant list" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const h = try mintChannelWithReply("ghs_tok");
    defer for (h.fds) |fd| pipe_proto.testOsClose(fd);

    const sm = try buildSecrets(arena); // carries static `fly`
    var resolver = credential_request.MintResolver{
        .mintable = &.{.{ .name = "github", .integration = "github" }},
        .channel = h.ch,
    };
    // `fly` is NOT in the grant list → static lookup; `github` IS → mint.
    const out = try substitute(arena, "fly=${secrets.fly.api_token} gh=${secrets.github.token}", sm, &resolver);
    try std.testing.expectEqualStrings("fly=FlyTokenXyz gh=ghs_tok", out);
}

test "substitute fails closed: a mintable name with no wired channel aborts" {
    var resolver = credential_request.MintResolver{
        .mintable = &.{.{ .name = "github", .integration = "github" }},
        .channel = null, // no session channel
    };
    try std.testing.expectError(error.MintUnavailable, substitute(std.testing.allocator, "${secrets.github.token}", null, &resolver));
}

test "substitute rejects a non-token field on a mintable credential" {
    var resolver = credential_request.MintResolver{
        .mintable = &.{.{ .name = "github", .integration = "github" }},
        .channel = null,
    };
    // A mintable handle has no static fields; only `.token` is valid.
    try std.testing.expectError(error.MissingField, substitute(std.testing.allocator, "${secrets.github.installation_id}", null, &resolver));
}
