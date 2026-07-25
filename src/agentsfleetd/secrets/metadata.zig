//! Pure, DB-free projection of a credential body into the non-secret descriptors
//! the list API exposes. Classification is a server fact — the client reads
//! `kind`, it never re-guesses from the user-chosen name — and the `api_key`
//! value is NEVER projected: `Projection` carries only the boolean `has_key`,
//! so leaking the key itself is a compile error rather than a review catch.
//!
//! Lives under `secrets/` rather than beside a handler because the metadata promotion made
//! this a WRITE-time function. `state/vault.zig` runs it once as a credential is
//! stored and persists the result to the `meta_*` columns
//! (`schema/036_vault_secret_metadata.sql`); read paths then select those columns
//! instead of opening an envelope per row. A projector that both the storage
//! layer and the HTTP layer call cannot live inside the HTTP layer — `state/`
//! importing from `http/handlers/` would invert the dependency direction.
//!
//! Leaf module: imports `std` only. Everything here is pure over a borrowed
//! `std.json.Value`, so every classification and projection branch is
//! unit-tested with no database and no decrypt.

const std = @import("std");

const S_PROVIDER = "provider";
const S_MODEL = "model";
const S_BASE_URL = "base_url";
const S_API_KEY = "api_key";

/// Provider id in the self-managed credential JSON that opts the credential into
/// a custom OpenAI-compatible endpoint — `base_url` is required iff the provider
/// equals this, and forbidden otherwise (RULE UFS; the runner uses the distinct
/// `custom:<url>` wire name, never this id, when dialing nullclaw).
///
/// Canonical home. `state/secret_probe.zig` re-exports it so the existing
/// `tenant_provider` chain is unchanged; classification is the reason the
/// constant exists, so it belongs beside `classify`.
pub const OPENAI_COMPATIBLE_PROVIDER: []const u8 = "openai-compatible";

/// What a stored credential *is*, derived from its `provider` field. The wire
/// value is the `@tagName` and is kept verbatim in the TS client union (the
/// cross-runtime half of RULE UFS), so a rename here is a wire break there.
pub const Kind = enum {
    provider_key,
    custom_endpoint,
    custom_secret,

    pub fn wire(self: Kind) []const u8 {
        return @tagName(self);
    }
};

/// Non-secret descriptors borrowed from the parsed body. Every slice points
/// into the caller's `std.json.Parsed` arena — dupe before that arena is freed.
///
/// There is intentionally no `api_key` field. `has_key` answers the only
/// question a caller is entitled to ask about the key — whether one is set —
/// and answering it with a `bool` means no code path can widen it into the
/// value. This is the type that `state/vault.zig` persists to the `meta_*`
/// columns, so the columns inherit the same guarantee: the table cannot hold a
/// key it has no column for.
pub const Projection = struct {
    kind: Kind,
    provider: ?[]const u8 = null,
    model: ?[]const u8 = null,
    base_url: ?[]const u8 = null,
    has_key: bool = false,
};

/// Classify by the `provider` field, never the user-chosen name: a missing or
/// non-string provider is an opaque `custom_secret`; the openai-compatible id
/// is a `custom_endpoint`; any other provider id is a `provider_key`. (A custom
/// secret that happens to carry a string `provider` misfiles as a provider key
/// — the accepted MVP edge, spec §Product-Clarity.)
pub fn classify(value: std.json.Value) Kind {
    if (value != .object) return .custom_secret;
    const provider_v = value.object.get(S_PROVIDER) orelse return .custom_secret;
    if (provider_v != .string) return .custom_secret;
    if (std.mem.eql(u8, provider_v.string, OPENAI_COMPATIBLE_PROVIDER))
        return .custom_endpoint;
    return .provider_key;
}

/// Project the non-secret descriptors for `value`'s kind. Slices are borrowed
/// from `value`; the `api_key` VALUE is never read out, only tested for
/// presence. A `custom_secret` carries no descriptors; a `provider_key` never
/// carries a `base_url`.
///
/// `has_key` is computed for every kind, including `custom_secret`: an opaque
/// credential can still hold an `api_key`, and the Models page reports presence
/// for it the same way. Only the descriptors are kind-dependent.
pub fn project(value: std.json.Value) Projection {
    const kind = classify(value);
    const has_key = hasNonEmptyApiKey(value);
    return switch (kind) {
        .custom_secret => .{ .kind = kind, .has_key = has_key },
        .provider_key => .{
            .kind = kind,
            .provider = optString(value, S_PROVIDER),
            .model = optString(value, S_MODEL),
            .has_key = has_key,
        },
        .custom_endpoint => .{
            .kind = kind,
            .provider = optString(value, S_PROVIDER),
            .model = optString(value, S_MODEL),
            .base_url = optString(value, S_BASE_URL),
            .has_key = has_key,
        },
    };
}

/// Whether the body carries a non-empty `api_key` string. The value is compared
/// against zero length and then dropped — it is never returned, logged, or
/// copied. Moved here from `handlers/tenant_model_entries_view.zig` (the metadata promotion) so that all four persisted `meta_*` values are computed by one function
/// at one moment, from one parse of one body. Two producers is how the stored
/// projection drifts from the blob it describes.
pub fn hasNonEmptyApiKey(value: std.json.Value) bool {
    if (value != .object) return false;
    const v = value.object.get(S_API_KEY) orelse return false;
    return v == .string and v.string.len > 0;
}

fn optString(value: std.json.Value, field: []const u8) ?[]const u8 {
    const v = value.object.get(field) orelse return null;
    return if (v == .string) v.string else null;
}

// ── tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

fn parse(json: []const u8) !std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, testing.allocator, json, .{});
}

test "classify keys on the provider field, not the credential name" {
    // A named provider id → provider_key, regardless of the (user-chosen) name.
    {
        const p = try parse(
            \\{"provider":"anthropic","api_key":"sk-x","model":"claude-sonnet-4-6"}
        );
        defer p.deinit();
        try testing.expectEqual(Kind.provider_key, classify(p.value));
    }
    // The openai-compatible sentinel → custom_endpoint.
    {
        const p = try parse(
            \\{"provider":"openai-compatible","base_url":"https://h/v1","model":"m","api_key":"k"}
        );
        defer p.deinit();
        try testing.expectEqual(Kind.custom_endpoint, classify(p.value));
    }
    // No provider field → opaque custom_secret.
    {
        const p = try parse(
            \\{"host":"api.machines.dev","api_token":"t"}
        );
        defer p.deinit();
        try testing.expectEqual(Kind.custom_secret, classify(p.value));
    }
    // A non-string provider is not a classification signal → custom_secret.
    {
        const p = try parse(
            \\{"provider":123,"model":"m"}
        );
        defer p.deinit();
        try testing.expectEqual(Kind.custom_secret, classify(p.value));
    }
    // A non-object body (legacy/corrupt) degrades to custom_secret.
    {
        const p = try parse(
            \\["not","an","object"]
        );
        defer p.deinit();
        try testing.expectEqual(Kind.custom_secret, classify(p.value));
    }
}

test "project extracts the kind's non-secret descriptors and never the api_key" {
    // provider_key: provider + model, never a base_url.
    {
        const p = try parse(
            \\{"provider":"anthropic","api_key":"sk-secret","model":"claude-sonnet-4-6"}
        );
        defer p.deinit();
        const got = project(p.value);
        try testing.expectEqual(Kind.provider_key, got.kind);
        try testing.expectEqualStrings("anthropic", got.provider.?);
        try testing.expectEqualStrings("claude-sonnet-4-6", got.model.?);
        try testing.expect(got.base_url == null);
        // Projection has no api_key field — the secret cannot be carried out.
        try testing.expect(!@hasField(Projection, "api_key"));
    }
    // custom_endpoint: provider + model + base_url.
    {
        const p = try parse(
            \\{"provider":"openai-compatible","base_url":"https://gw/v1","model":"kimi","api_key":"k"}
        );
        defer p.deinit();
        const got = project(p.value);
        try testing.expectEqual(Kind.custom_endpoint, got.kind);
        try testing.expectEqualStrings("openai-compatible", got.provider.?);
        try testing.expectEqualStrings("kimi", got.model.?);
        try testing.expectEqualStrings("https://gw/v1", got.base_url.?);
    }
    // custom_secret: no descriptors at all.
    {
        const p = try parse(
            \\{"host":"h","api_token":"t"}
        );
        defer p.deinit();
        const got = project(p.value);
        try testing.expectEqual(Kind.custom_secret, got.kind);
        try testing.expect(got.provider == null);
        try testing.expect(got.model == null);
        try testing.expect(got.base_url == null);
    }
    // A provider_key missing its model degrades that one field to null, not the kind.
    {
        const p = try parse(
            \\{"provider":"openai","api_key":"k"}
        );
        defer p.deinit();
        const got = project(p.value);
        try testing.expectEqual(Kind.provider_key, got.kind);
        try testing.expectEqualStrings("openai", got.provider.?);
        try testing.expect(got.model == null);
    }
}

test "wire value matches the enum tag verbatim (TS union parity)" {
    try testing.expectEqualStrings("provider_key", Kind.provider_key.wire());
    try testing.expectEqualStrings("custom_endpoint", Kind.custom_endpoint.wire());
    try testing.expectEqualStrings("custom_secret", Kind.custom_secret.wire());
}

test "has_key reports presence for every kind, never the key itself" {
    // A named provider with a key.
    {
        const p = try parse(
            \\{"provider":"anthropic","api_key":"sk-live-abcdef"}
        );
        defer p.deinit();
        const got = project(p.value);
        try testing.expectEqual(Kind.provider_key, got.kind);
        try testing.expect(got.has_key);
    }
    // A keyless openai-compatible gateway is valid and must report false rather
    // than being treated as malformed — the optional-key design.
    {
        const p = try parse(
            \\{"provider":"openai-compatible","base_url":"https://h/v1"}
        );
        defer p.deinit();
        const got = project(p.value);
        try testing.expectEqual(Kind.custom_endpoint, got.kind);
        try testing.expect(!got.has_key);
    }
    // An opaque credential still answers presence: `custom_secret` is a
    // classification, not an exemption from the question.
    {
        const p = try parse(
            \\{"host":"api.machines.dev","api_key":"tok"}
        );
        defer p.deinit();
        const got = project(p.value);
        try testing.expectEqual(Kind.custom_secret, got.kind);
        try testing.expect(got.has_key);
    }
}

test "has_key is false for empty, absent, and non-string keys" {
    // Empty string is "no key", not "a key of length zero" — an operator who
    // cleared the field must not see the row claim a key is configured.
    {
        const p = try parse(
            \\{"provider":"openai","api_key":""}
        );
        defer p.deinit();
        try testing.expect(!project(p.value).has_key);
    }
    // Absent entirely.
    {
        const p = try parse(
            \\{"provider":"openai"}
        );
        defer p.deinit();
        try testing.expect(!project(p.value).has_key);
    }
    // A non-string api_key is malformed input, not a present key. Reporting
    // `true` here would tell the dashboard a key is set that nothing can use.
    {
        const p = try parse(
            \\{"provider":"openai","api_key":12345}
        );
        defer p.deinit();
        try testing.expect(!project(p.value).has_key);
    }
    // A non-object body cannot carry a key at all.
    {
        const p = try parse("\"just-a-string\"");
        defer p.deinit();
        try testing.expect(!project(p.value).has_key);
        try testing.expectEqual(Kind.custom_secret, project(p.value).kind);
    }
}

test "the projection has no field capable of carrying the key value" {
    // Structural, not behavioural: this is the guarantee the `meta_*` columns
    // inherit. If someone adds an `api_key` field to Projection, this fails at
    // COMPILE time via the field-name scan below rather than waiting for a
    // reviewer to notice a leak.
    inline for (@typeInfo(Projection).@"struct".fields) |f| {
        try testing.expect(!std.mem.eql(u8, f.name, "api_key"));
        try testing.expect(!std.mem.eql(u8, f.name, "secret"));
        try testing.expect(!std.mem.eql(u8, f.name, "token"));
    }
}
