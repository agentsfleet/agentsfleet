//! §4 Dimension 4.1 — allowed credential metadata, enforced structurally.
//!
//! Spec: *"Secret/credential metadata carried by authenticated HTTP/UI is
//! limited to canonical `secret_ref`, provider, kind, base URL, `has_key`,
//! required/failing credential names, and presence booleans ... Encrypted
//! ciphertext may persist only in the vault; secret values / API keys never
//! leave securely erased trusted Zig memory."*
//!
//! ## Why a type-level check rather than a runtime scan
//!
//! The obvious test drives a request with a sentinel secret and greps the
//! response for it. That catches a leak only on the path the test happens to
//! exercise, with the value the test happens to choose — a new field on a
//! different endpoint sails past.
//!
//! These assertions run over the STRUCT DEFINITIONS with `@typeInfo`, so they
//! cover every field that exists rather than every field someone remembered to
//! exercise. Adding `api_key` to the response projection fails at compile-test
//! time, whether or not anyone wrote a request test for it.
//!
//! The two checks are deliberately different in kind:
//!
//!   - the DENY list catches a secret-shaped field arriving under a name we can
//!     recognise (`api_key`, `token`, `password`);
//!   - the ALLOW list catches one arriving under a name we cannot — any new
//!     credential-derived field at all has to be added here consciously, which
//!     is the point where someone has to decide whether it is metadata or a
//!     secret.
//!
//! A deny list alone would miss `credential_blob`. An allow list is what makes
//! the default "refuse".

const std = @import("std");

const metadata = @import("../../../secrets/metadata.zig");
const view = @import("../tenant_model_entries_view.zig");

const testing = std.testing;

/// Every field the response projection may carry that is DERIVED FROM a stored
/// credential. §4 fixes this set; anything outside it is either a non-secret
/// model-page field (listed separately below) or a leak.
const ALLOWED_CREDENTIAL_FIELDS = [_][]const u8{
    "secret_ref",
    "provider",
    "kind",
    "base_url",
    "has_key",
};

/// Non-secret model-page fields §4 also permits. They describe the catalogue
/// entry, not the credential — a model id and its published rates are the same
/// for every tenant and are not derived from anyone's stored key.
const ALLOWED_ENTRY_FIELDS = [_][]const u8{
    "id",
    "model_id",
    "context_cap_tokens",
    "input_nanos_per_mtok",
    "cached_input_nanos_per_mtok",
    "output_nanos_per_mtok",
    "active",
    "created_at",
};

/// Field names that would mean a secret VALUE had reached a response type.
/// Matched as substrings, so `api_key`, `apiKey`, `provider_api_key` and
/// `key_material` all trip it.
const FORBIDDEN_SUBSTRINGS = [_][]const u8{
    "api_key",
    "apikey",
    "password",
    "passphrase",
    "ciphertext",
    "plaintext",
    "key_material",
    "secret_value",
};

fn containsIgnoringCase(haystack: []const u8, needle: []const u8) bool {
    if (needle.len > haystack.len) return false;
    var i: usize = 0;
    outer: while (i + needle.len <= haystack.len) : (i += 1) {
        var j: usize = 0;
        while (j < needle.len) : (j += 1) {
            if (std.ascii.toLower(haystack[i + j]) != std.ascii.toLower(needle[j])) continue :outer;
        }
        return true;
    }
    return false;
}

fn isListed(name: []const u8, list: []const []const u8) bool {
    for (list) |entry| if (std.mem.eql(u8, name, entry)) return true;
    return false;
}

/// Assert no field of `T` carries a secret-shaped name.
fn expectNoForbiddenFields(comptime T: type) !void {
    inline for (@typeInfo(T).@"struct".fields) |field| {
        for (FORBIDDEN_SUBSTRINGS) |bad| {
            if (containsIgnoringCase(field.name, bad)) {
                std.debug.print(
                    "field '{s}' on {s} matches forbidden pattern '{s}'\n",
                    .{ field.name, @typeName(T), bad },
                );
                return error.ForbiddenFieldOnResponseType;
            }
        }
    }
}

test "test_library_secret_and_metadata_sink_policy" {
    // The response projection carries no secret-shaped field.
    try expectNoForbiddenFields(view.EntryView);

    // And nothing outside the two permitted sets. This is the half that catches
    // a leak arriving under a name a deny list would not recognise: a new field
    // must be classified here, deliberately, before it can ship.
    inline for (@typeInfo(view.EntryView).@"struct".fields) |field| {
        const allowed = isListed(field.name, &ALLOWED_CREDENTIAL_FIELDS) or
            isListed(field.name, &ALLOWED_ENTRY_FIELDS);
        if (!allowed) {
            std.debug.print(
                "EntryView field '{s}' is in neither allowed set — classify it as credential metadata or a non-secret entry field, or remove it\n",
                .{field.name},
            );
            return error.UnclassifiedResponseField;
        }
    }
}

test "test_library_secret_and_metadata_sink_policy: the write-time projection carries no secret" {
    // `metadata.Projection` is what `vault.storeJsonPlaintext` writes into the
    // meta_* columns beside the ciphertext. A secret reaching THIS type would be
    // persisted unencrypted in a column, which is worse than a response leak —
    // the response is transient, the column is not.
    try expectNoForbiddenFields(metadata.Projection);

    inline for (@typeInfo(metadata.Projection).@"struct".fields) |field| {
        // `model` is permitted here though it is not on the credential list: it
        // identifies the catalogue entry a key was registered against, not
        // anything about the key.
        const allowed = isListed(field.name, &ALLOWED_CREDENTIAL_FIELDS) or
            std.mem.eql(u8, field.name, "model");
        if (!allowed) {
            std.debug.print("Projection field '{s}' is unclassified\n", .{field.name});
            return error.UnclassifiedProjectionField;
        }
    }
}

test "test_library_secret_and_metadata_sink_policy: the forbidden matcher actually matches" {
    // A guard whose matcher is broken passes silently and proves nothing, so the
    // matcher itself is exercised on the shapes it is meant to catch.
    try testing.expect(containsIgnoringCase("api_key", "api_key"));
    try testing.expect(containsIgnoringCase("apiKey", "apikey"));
    try testing.expect(containsIgnoringCase("provider_api_key", "api_key"));
    try testing.expect(containsIgnoringCase("KEY_MATERIAL", "key_material"));
    try testing.expect(containsIgnoringCase("leading_ciphertext_blob", "ciphertext"));

    // And does not fire on the legitimate names, or the allow list would be
    // unsatisfiable and this whole file would be vacuous.
    try testing.expect(!containsIgnoringCase("secret_ref", "secret_value"));
    try testing.expect(!containsIgnoringCase("has_key", "api_key"));
    try testing.expect(!containsIgnoringCase("base_url", "password"));
}

test "test_library_secret_and_metadata_sink_policy: every allowed credential field really exists" {
    // Keeps the allow list honest in the other direction. A stale entry naming a
    // field that no longer exists would silently widen what a future field could
    // be called without review.
    inline for (ALLOWED_CREDENTIAL_FIELDS) |allowed| {
        var found = false;
        inline for (@typeInfo(view.EntryView).@"struct".fields) |field| {
            if (std.mem.eql(u8, field.name, allowed)) found = true;
        }
        if (!found) {
            std.debug.print("allow-list names '{s}', which EntryView no longer has\n", .{allowed});
            return error.StaleAllowList;
        }
    }
}
