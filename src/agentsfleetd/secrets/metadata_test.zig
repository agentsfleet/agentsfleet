//! Tests for the credential metadata projector.
//!
//! Split out under RULE FLL when the userinfo guard pushed `metadata.zig` past
//! 350 lines. The module stays the projector; this file stays its proof.

const std = @import("std");
const metadata = @import("metadata.zig");

const Kind = metadata.Kind;
const Projection = metadata.Projection;
const project = metadata.project;
const classify = metadata.classify;

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

test "a base_url carrying userinfo is never projected into the plaintext column" {
    // The regression guard for the promotion's blind spot. base_url_guard
    // ACCEPTS this URL — it validates the host and strips userinfo on the way —
    // so nothing upstream rejects a credential shaped like this, and before the
    // projector dropped it the password landed in `meta_base_url` where any
    // database reader could select it without the Key Encryption Key.
    // `openai-compatible` is the ONLY provider that classifies as
    // `custom_endpoint`, and `custom_endpoint` is the only kind that projects a
    // base_url at all — so it is the only shape that can carry this leak.
    const p = try parse(
        \\{"provider":"openai-compatible","base_url":"https://user:pw@gw.example.com:8443/v1","api_key":"sk-x"}
    );
    defer p.deinit();
    const projected = project(p.value);
    try testing.expectEqual(Kind.custom_endpoint, projected.kind);
    try testing.expect(projected.base_url == null);
    // The rest of the projection is unaffected — dropping the URL must not
    // quietly downgrade the row to an opaque secret or lose key presence.
    try testing.expect(projected.has_key);
    try testing.expectEqualStrings(metadata.OPENAI_COMPATIBLE_PROVIDER, projected.provider.?);
}

test "an ordinary base_url still projects, including an @ outside the authority" {
    // The other half: the guard drops URLs by AUTHORITY, not by the presence of
    // an `@` anywhere. A path or query carrying one is an ordinary endpoint, and
    // hiding it would cost real display for no security gain.
    {
        const p = try parse(
            \\{"provider":"openai-compatible","base_url":"https://gw.example.com:8443/v1","api_key":"sk-x"}
        );
        defer p.deinit();
        try testing.expectEqualStrings("https://gw.example.com:8443/v1", project(p.value).base_url.?);
    }
    {
        const p = try parse(
            \\{"provider":"openai-compatible","base_url":"https://gw.example.com/v1/tenants/a@b","api_key":"sk-x"}
        );
        defer p.deinit();
        try testing.expectEqualStrings("https://gw.example.com/v1/tenants/a@b", project(p.value).base_url.?);
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
