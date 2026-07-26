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
            .base_url = displayableBaseUrl(optString(value, S_BASE_URL)),
            .has_key = has_key,
        },
    };
}

/// The `base_url` as it may be persisted IN PLAINTEXT and shown to callers, or
/// null when it may not be.
///
/// `schema/036` moved this value out of the AES-GCM envelope into a column, on
/// the reasoning that every projected field is metadata any authorized caller
/// already sees. That holds for a scheme, host, port and path. It does not hold
/// for `https://user:pw@host/v1` — `state/base_url_guard.zig` validates the
/// HOST and deliberately accepts userinfo (its own test asserts that
/// `https://user:pw@gw.example.com:8443/v1` is `.ok`), so a credential can carry
/// a password inside its URL. Promoting that string verbatim converts a
/// KEK-protected secret into one any database reader can `SELECT`, which is the
/// opposite of what the promotion was argued on.
///
/// Omitted rather than rewritten. Stripping `user:pw@` from the middle of a URL
/// produces a string that is not a subslice of the input, and this projector is
/// deliberately allocation-free so that one parse of one body yields every
/// persisted value. A credential-bearing `base_url` is a misconfiguration; a
/// page showing no endpoint for it is a better outcome than a page — and a
/// column — showing the password.
fn displayableBaseUrl(raw: ?[]const u8) ?[]const u8 {
    const url = raw orelse return null;
    const sep = std.mem.indexOf(u8, url, "://") orelse return url;
    const after_scheme = url[sep + 3 ..];
    // Only the AUTHORITY is examined. A `@` after the authority is an ordinary
    // path or query byte, and dropping those URLs would hide legitimate
    // endpoints for no gain.
    const authority_end = std.mem.indexOfAny(u8, after_scheme, "/?#") orelse after_scheme.len;
    if (std.mem.indexOfScalar(u8, after_scheme[0..authority_end], '@') != null) return null;
    return url;
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

test {
    _ = @import("metadata_test.zig");
}
