//! Cache-key construction and handle/binding fingerprinting for the credential
//! broker. Split from `broker.zig` for the file-length budget (RULE FLL); the
//! broker owns lifecycle and dispatch, this owns "what makes two mints the same
//! cache entry".
//!
//! That question is load-bearing. A key that omits a component silently serves
//! one caller's token to another — see `bindingFingerprint` for the case
//! where two fleets in one workspace share an installation handle.

const std = @import("std");
const integration = @import("integration.zig");

/// Cache-key separator joining (workspace, integration, identity, binding).
/// ASCII unit separator — never present in any field, so key boundaries cannot
/// collide.
pub const KEY_SEP: u8 = 0x1f;

/// Hex width of a fingerprint appended to the cache key.
pub const FP_HEX_LEN: usize = @sizeOf(u64) * 2;

/// Fixed-width lower-hex format for a fingerprint. Fixed width keeps the key
/// length predictable and every byte printable (RULE UFS — both fingerprints
/// in the key must render identically or two keys could differ only in padding).
const FP_HEX_FMT = "{x:0>16}";

/// Fingerprint standing for "this fleet declared no repository binding". A fixed
/// non-zero constant so it cannot be confused with a real hash landing on zero.
pub const NO_BINDING_FP: u64 = 0xFFFF_FFFF_FFFF_FFFF;

pub fn writeKey(buf: []u8, workspace: []const u8, id_name: []const u8, fingerprint: u64, binding_fp: u64) ?[]const u8 {
    if (workspace.len + id_name.len + 3 + FP_HEX_LEN * 2 > buf.len) return null;
    @memcpy(buf[0..workspace.len], workspace);
    buf[workspace.len] = KEY_SEP;
    @memcpy(buf[workspace.len + 1 ..][0..id_name.len], id_name);
    var pos = workspace.len + 1 + id_name.len;
    buf[pos] = KEY_SEP;
    pos += 1;
    // Fixed-width hex keeps the key length predictable and the bytes printable.
    const fp_hex = std.fmt.bufPrint(buf[pos..], FP_HEX_FMT, .{fingerprint}) catch return null;
    pos += fp_hex.len;
    buf[pos] = KEY_SEP;
    pos += 1;
    const binding_hex = std.fmt.bufPrint(buf[pos..], FP_HEX_FMT, .{binding_fp}) catch return null;
    return buf[0 .. pos + binding_hex.len];
}

/// 64-bit fingerprint of a repository binding — the repositories it names and the
/// access level it grants. Order-sensitive by design: two bindings listing the
/// same repositories differently are different cache entries, which costs one
/// extra mint and never risks serving the wrong scope. A null binding folds to a
/// distinct constant so "declared nothing" and "declared something" never collide.
///
/// Framed with `hashFramed` plus an explicit count — exactly the discipline
/// `hashValue`'s array arm already uses — and NOT separator-joined. A separator
/// only frames input it cannot appear in, and nothing validates repository
/// strings before they reach here (`fleet_runtime/config_repositories.zig` dupes
/// whatever the frontmatter declared). Joining on `KEY_SEP` therefore made
/// `["acme/a","acme/b"]` and the single entry `"acme/a<KEY_SEP>acme/b"` hash
/// IDENTICALLY: a deterministic alias needing no probabilistic collision. The
/// fleet declaring the second string would be served the first fleet's cached
/// broad-scope token — the exact cross-fleet bleed this fingerprint exists to
/// stop, reintroduced one layer above the mint that would have refused it.
///
/// `seed` is the broker's per-process value, so a digest cannot be precomputed
/// offline against a known target either.
pub fn bindingFingerprint(seed: u64, binding: ?integration.RepositoryBinding) u64 {
    const b = binding orelse return NO_BINDING_FP;
    var h = std.hash.Wyhash.init(seed);
    hashFramed(&h, @tagName(b.access));
    h.update(std.mem.asBytes(&b.repositories.len));
    for (b.repositories) |repo| hashFramed(&h, repo);
    return h.final();
}

/// 64-bit fingerprint of the handle's STABLE identity: every top-level field
/// except the rotating-credential set (`integration.ROTATING_CREDENTIAL_FIELDS`).
/// An ordinary refresh-token rotation keeps the fingerprint (cache hit); a
/// reconnect misses ONLY because at least one non-excluded field changed —
/// which the connect callbacks guarantee by stamping `connected_at_ms` on every
/// stored handle (a refresh provider's other identity fields can be constants).
/// Non-object handles (rejected upstream by `parseIntegration`) hash their
/// raw value defensively.
pub fn identityFingerprint(seed: u64, handle: std.json.Value) u64 {
    var hasher = std.hash.Wyhash.init(seed);
    switch (handle) {
        .object => |obj| hashObject(&hasher, obj, true),
        else => hashValue(&hasher, handle),
    }
    return hasher.final();
}

/// Hash `obj` in canonical (ascending key) order via an allocation-free
/// selection walk, so JSON parser/insertion order cannot change the result.
/// Every key and string value is length-framed so adjacent fields cannot
/// alias across boundaries ({"a":"xb","c":…} vs {"a":"x","bc":…}).
/// `exclude_rotating` drops the rotating-credential fields (top level only).
fn hashObject(hasher: *std.hash.Wyhash, obj: std.json.ObjectMap, exclude_rotating: bool) void {
    var prev: ?[]const u8 = null;
    while (nextKeyAfter(obj, prev, exclude_rotating)) |key| {
        hashFramed(hasher, key);
        hashValue(hasher, obj.get(key).?);
        prev = key;
    }
}

/// Length-prefix + bytes: the injective framing for variable-length pieces.
fn hashFramed(hasher: *std.hash.Wyhash, bytes: []const u8) void {
    hasher.update(std.mem.asBytes(&bytes.len));
    hasher.update(bytes);
}

/// The smallest key strictly greater than `prev` (null → the smallest key),
/// skipping excluded fields. O(n²) over a vault handle's handful of fields —
/// cheaper than allocating and sorting a key list on the mint hot path.
fn nextKeyAfter(obj: std.json.ObjectMap, prev: ?[]const u8, exclude_rotating: bool) ?[]const u8 {
    var best: ?[]const u8 = null;
    var it = obj.iterator();
    while (it.next()) |e| {
        const k = e.key_ptr.*;
        if (exclude_rotating and isRotatingField(k)) continue;
        if (prev) |p| {
            if (std.mem.order(u8, k, p) != .gt) continue;
        }
        if (best == null or std.mem.order(u8, k, best.?) == .lt) best = k;
    }
    return best;
}

fn isRotatingField(name: []const u8) bool {
    for (integration.ROTATING_CREDENTIAL_FIELDS) |f| {
        if (std.mem.eql(u8, name, f)) return true;
    }
    return false;
}

/// Hash a JSON value with a leading type tag, so `"5"` and `5` (or `null` and
/// an empty string) cannot collide. Arrays keep their order (order is
/// meaningful); nested objects re-canonicalize but never exclude (the rotating
/// exclusion applies at the handle's top level only).
fn hashValue(hasher: *std.hash.Wyhash, v: std.json.Value) void {
    hasher.update(&[_]u8{@intFromEnum(std.meta.activeTag(v))});
    switch (v) {
        .null => {},
        .bool => |b| hasher.update(&[_]u8{@intFromBool(b)}),
        .integer => |n| hasher.update(std.mem.asBytes(&n)),
        .float => |f| hasher.update(std.mem.asBytes(&f)),
        .number_string, .string => |s| hashFramed(hasher, s),
        .array => |arr| {
            hasher.update(std.mem.asBytes(&arr.items.len));
            for (arr.items) |item| hashValue(hasher, item);
        },
        .object => |obj| hashObject(hasher, obj, false),
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

/// `"acme/a" KEY_SEP "acme/b"` as ONE repository string. Written byte-wise so the
/// separator inside it is impossible to miss when reading the test.
const SPLICED_NAME = [_]u8{ 'a', 'c', 'm', 'e', '/', 'a', KEY_SEP, 'a', 'c', 'm', 'e', '/', 'b' };

test "bindingFingerprint: a separator inside a repository name cannot alias two entries into one" {
    // The regression this guards: joining on KEY_SEP without length framing made
    // these two bindings hash identically, so a fleet declaring the spliced
    // spelling was served the two-repository fleet's cached broad-scope token —
    // above the mint that would have refused the malformed name. Nothing
    // validates repository strings before this point, so it is authorable.
    const two = [_][]const u8{ "acme/a", "acme/b" };
    const spliced = [_][]const u8{&SPLICED_NAME};

    const fp_two = bindingFingerprint(0, .{ .repositories = &two, .access = .read });
    const fp_spliced = bindingFingerprint(0, .{ .repositories = &spliced, .access = .read });
    try testing.expect(fp_two != fp_spliced);
}

test "bindingFingerprint: access level, repository set, and count each move the digest" {
    const one = [_][]const u8{"acme/widgets"};
    const other = [_][]const u8{"acme/gadgets"};
    const two = [_][]const u8{ "acme/widgets", "acme/gadgets" };

    const read: integration.RepositoryBinding = .{ .repositories = &one, .access = .read };
    const write: integration.RepositoryBinding = .{ .repositories = &one, .access = .write };
    const other_read: integration.RepositoryBinding = .{ .repositories = &other, .access = .read };
    const two_read: integration.RepositoryBinding = .{ .repositories = &two, .access = .read };

    try testing.expect(bindingFingerprint(0, read) != bindingFingerprint(0, write));
    try testing.expect(bindingFingerprint(0, read) != bindingFingerprint(0, other_read));
    try testing.expect(bindingFingerprint(0, read) != bindingFingerprint(0, two_read));
    // "declared nothing" is a constant, so it can never be reached by hashing.
    try testing.expectEqual(NO_BINDING_FP, bindingFingerprint(0, null));
}

test "bindingFingerprint: the per-process seed changes the digest" {
    // Same input, different broker process → different key, so a digest cannot be
    // precomputed offline against a known target binding.
    const one = [_][]const u8{"acme/widgets"};
    const b: integration.RepositoryBinding = .{ .repositories = &one, .access = .write };
    try testing.expect(bindingFingerprint(1, b) != bindingFingerprint(2, b));
}
