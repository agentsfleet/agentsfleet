//! Pure, DB-free projection of a decrypted credential body into the non-secret
//! descriptors the list API exposes (§1). Classification is a server
//! fact — the client reads `kind`, it never re-guesses from the user-chosen
//! name — and the `api_key` is NEVER read here: `Projection` has no field for
//! it, so a leak is a compile error, not a review catch. Split out of the
//! handler so every classification + projection branch is unit-tested with no
//! DB and no decrypt.

const std = @import("std");
const secret_probe = @import("../../../state/secret_probe.zig");

const S_PROVIDER = "provider";
const S_MODEL = "model";
const S_BASE_URL = "base_url";

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
/// There is intentionally no `api_key` field: the secret is never projected.
pub const Projection = struct {
    kind: Kind,
    provider: ?[]const u8 = null,
    model: ?[]const u8 = null,
    base_url: ?[]const u8 = null,
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
    if (std.mem.eql(u8, provider_v.string, secret_probe.OPENAI_COMPATIBLE_PROVIDER))
        return .custom_endpoint;
    return .provider_key;
}

/// Project the non-secret descriptors for `value`'s kind. Slices are borrowed
/// from `value`; the `api_key` is never touched. A `custom_secret` carries no
/// descriptors; a `provider_key` never carries a `base_url`.
pub fn project(value: std.json.Value) Projection {
    const kind = classify(value);
    return switch (kind) {
        .custom_secret => .{ .kind = kind },
        .provider_key => .{
            .kind = kind,
            .provider = optString(value, S_PROVIDER),
            .model = optString(value, S_MODEL),
        },
        .custom_endpoint => .{
            .kind = kind,
            .provider = optString(value, S_PROVIDER),
            .model = optString(value, S_MODEL),
            .base_url = optString(value, S_BASE_URL),
        },
    };
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
