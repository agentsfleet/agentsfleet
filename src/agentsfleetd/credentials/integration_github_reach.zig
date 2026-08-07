//! Does the token GitHub actually minted reach exactly what the fleet declared?
//!
//! The request body names repositories by BARE name, because GitHub scopes an
//! installation token by name within the installation's own account. A fleet's
//! binding is written `owner/repo`. So the owner never reaches the wire, and a
//! binding naming `acme/payments` mints happily against
//! `<installed-account>/payments` whenever a repository by that bare name exists
//! there. It cannot cross a tenant — an installation belongs to one account —
//! but it is a real mis-scope inside the operator's own installation.
//!
//! Nothing else on the mint path compares the qualified spelling, so without
//! this check the declaration and the credential could quietly disagree. This
//! module is the one place that makes them agree.
//!
//! It checks the RESPONSE rather than re-deriving the request. A
//! create-installation-access-token response echoes a `repositories` array
//! carrying the qualified `full_name` of every repository the token reaches, so
//! comparing that set against the declared set validates what the credential can
//! actually touch — including a mis-scope whose cause nobody modelled. The
//! comparison is case-insensitive because GitHub owners and repository names are.

const std = @import("std");
const ctx = @import("integration_ctx.zig");
const github_body = @import("integration_github_body.zig");

/// Response fields naming the reach (RULE UFS — the response-side counterparts
/// of the request field names in `integration_github.zig`).
const RESP_FIELD_REPOSITORIES: []const u8 = "repositories";
const RESP_FIELD_FULL_NAME: []const u8 = "full_name";
const RESP_FIELD_PERMISSIONS: []const u8 = "permissions";

/// What the minted token turned out to reach, relative to the declaration.
/// A bare enum rather than a union: each variant carries its whole meaning, and
/// the caller refuses on anything that is not `.exact`.
pub const Verdict = enum {
    /// Reach and declaration are the same set. The only verdict that mints.
    exact,
    /// The token reaches a repository that was never declared, or misses one
    /// that was — the owner-stripping mis-scope this module exists to catch.
    mismatched,
    /// The response described no reach at all. Refused rather than assumed: a
    /// request that named repositories is answered with the ones it was granted,
    /// so a missing or malformed array means something happened that this code
    /// does not model — and "unknown reach" must never be the permissive branch.
    unstated,
};

/// Compare the reach a response states against the repositories a fleet declared.
/// Pure — no allocation and no I/O — so the property is a unit test rather than
/// an observation about a live installation.
pub fn verify(declared: []const []const u8, response: std.json.ObjectMap) Verdict {
    const field = response.get(RESP_FIELD_REPOSITORIES) orelse return .unstated;
    const reached = switch (field) {
        .array => |a| a.items,
        else => return .unstated,
    };
    // An empty binding never reaches here — the mint refuses one before the
    // exchange — but a response claiming zero reach against a real declaration
    // is a mismatch, not a pass.
    if (reached.len == 0) return if (declared.len == 0) .exact else .mismatched;

    // Direction one: nothing unnamed. This is the direction that catches the
    // mis-scope — a token for `<installed-account>/payments` against a binding
    // naming `acme/payments` reaches a repository the fleet never wrote down.
    for (reached) |entry| {
        const full_name = fullNameOf(entry) orelse return .unstated;
        if (!containsIgnoreCase(declared, full_name)) return .mismatched;
    }
    // Direction two: nothing missing, so a token silently NARROWER than the
    // declaration is refused here too, rather than failing later at the vendor
    // where the fleet has no local explanation for it.
    for (declared) |want| {
        if (!reaches(reached, want)) return .mismatched;
    }
    return .exact;
}

/// Compare the PERMISSIONS a response states against what the access level
/// requested. Load-bearing the moment write mints exist: a token carrying more
/// write than the fleet declared is the overreach this check refuses.
///
/// Rules, strict in the direction that matters:
///   * every requested permission must be present at exactly the requested
///     value — `contents` at the access level, `pull_requests: write` only at
///     write (its absence IS the read scope);
///   * no granted permission may exceed read unless it was requested — GitHub
///     attaches ambient read grants (`metadata`) to every installation token,
///     so read-level extras pass, write/admin-level extras refuse;
///   * a missing or malformed `permissions` object is `unstated`, refused —
///     unknown reach must never be the permissive branch.
pub fn verifyPermissions(access: ctx.RepositoryAccess, response: std.json.ObjectMap) Verdict {
    const field = response.get(RESP_FIELD_PERMISSIONS) orelse return .unstated;
    const perms = switch (field) {
        .object => |o| o,
        else => return .unstated,
    };

    const contents_want = switch (access) {
        .read => github_body.PERM_VALUE_READ,
        .write => github_body.PERM_VALUE_WRITE,
    };

    // Scan first, so a value this code does not model (a non-string) is
    // `unstated` — shape failure — rather than surfacing as a level mismatch.
    var it = perms.iterator();
    while (it.next()) |entry| {
        const key = entry.key_ptr.*;
        const value = switch (entry.value_ptr.*) {
            .string => |s| s,
            else => return .unstated,
        };
        if (std.mem.eql(u8, value, github_body.PERM_VALUE_READ)) continue;
        const requested_at_this_level =
            (std.mem.eql(u8, key, github_body.PERM_CONTENTS) and std.mem.eql(u8, value, contents_want)) or
            (access == .write and std.mem.eql(u8, key, github_body.PERM_PULL_REQUESTS) and std.mem.eql(u8, value, github_body.PERM_VALUE_WRITE));
        if (!requested_at_this_level) return .mismatched;
    }

    if (!permissionIs(perms, github_body.PERM_CONTENTS, contents_want)) return .mismatched;
    if (access == .write and !permissionIs(perms, github_body.PERM_PULL_REQUESTS, github_body.PERM_VALUE_WRITE)) return .mismatched;
    return .exact;
}

fn permissionIs(perms: std.json.ObjectMap, key: []const u8, want: []const u8) bool {
    const value = switch (perms.get(key) orelse return false) {
        .string => |s| s,
        else => return false,
    };
    return std.mem.eql(u8, value, want);
}

fn fullNameOf(entry: std.json.Value) ?[]const u8 {
    const obj = switch (entry) {
        .object => |o| o,
        else => return null,
    };
    const field = obj.get(RESP_FIELD_FULL_NAME) orelse return null;
    return switch (field) {
        .string => |s| s,
        else => null,
    };
}

fn containsIgnoreCase(haystack: []const []const u8, needle: []const u8) bool {
    for (haystack) |candidate| {
        if (std.ascii.eqlIgnoreCase(candidate, needle)) return true;
    }
    return false;
}

fn reaches(entries: []const std.json.Value, want: []const u8) bool {
    for (entries) |entry| {
        const full_name = fullNameOf(entry) orelse continue;
        if (std.ascii.eqlIgnoreCase(full_name, want)) return true;
    }
    return false;
}

// ── Tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

/// Parse a response body and run `verify` against it. The parsed document is
/// released before the verdict is returned, which is safe only because `Verdict`
/// borrows nothing from it — the property this helper also pins.
fn verdictOf(declared: []const []const u8, body: []const u8) !Verdict {
    var parsed = try std.json.parseFromSlice(std.json.Value, testing.allocator, body, .{});
    defer parsed.deinit();
    return verify(declared, parsed.value.object);
}

test "test_mint_reach_refuses_a_stripped_owner" {
    // THE regression. The binding names `acme/payments`; the owner is stripped at
    // the wire edge, so GitHub answers with the installation account's own
    // `megam/payments`. Nothing downstream would have noticed.
    const declared = [_][]const u8{"acme/payments"};
    const body =
        \\{"token":"ghs_x","repositories":[{"full_name":"megam/payments"}]}
    ;
    try testing.expectEqual(Verdict.mismatched, try verdictOf(&declared, body));
}

test "reach: a token reaching exactly the declared repositories is exact" {
    const declared = [_][]const u8{ "acme/payments", "acme/widgets" };
    const body =
        \\{"token":"ghs_x","repositories":[{"full_name":"acme/payments"},{"full_name":"acme/widgets"}]}
    ;
    try testing.expectEqual(Verdict.exact, try verdictOf(&declared, body));
}

test "reach: owner and repository names compare case-insensitively, as GitHub treats them" {
    const declared = [_][]const u8{"Acme/Payments"};
    const body =
        \\{"token":"ghs_x","repositories":[{"full_name":"acme/payments"}]}
    ;
    try testing.expectEqual(Verdict.exact, try verdictOf(&declared, body));
}

test "reach: a repository the fleet never declared is a mismatch" {
    // Breadth, rather than the wrong owner: the token also reaches something else
    // in the same account.
    const declared = [_][]const u8{"acme/payments"};
    const body =
        \\{"token":"ghs_x","repositories":[{"full_name":"acme/payments"},{"full_name":"acme/secrets"}]}
    ;
    try testing.expectEqual(Verdict.mismatched, try verdictOf(&declared, body));
}

test "reach: a declared repository missing from the reach is a mismatch" {
    // A token narrower than asked for. Refused locally so the failure names the
    // binding, instead of surfacing later as an opaque 404 from the vendor.
    const declared = [_][]const u8{ "acme/payments", "acme/widgets" };
    const body =
        \\{"token":"ghs_x","repositories":[{"full_name":"acme/payments"}]}
    ;
    try testing.expectEqual(Verdict.mismatched, try verdictOf(&declared, body));
}

test "reach: a response stating no reach is refused, never read as all-of-them" {
    const declared = [_][]const u8{"acme/payments"};
    // The pre-narrowing shape: a bare token with no repositories array. Reading
    // it as "everything" is exactly the behaviour the mint narrowing removed.
    try testing.expectEqual(Verdict.unstated, try verdictOf(&declared, "{\"token\":\"ghs_x\"}"));
    // Present but not an array, and present but empty.
    try testing.expectEqual(Verdict.unstated, try verdictOf(&declared, "{\"repositories\":\"all\"}"));
    try testing.expectEqual(Verdict.mismatched, try verdictOf(&declared, "{\"repositories\":[]}"));
}

test "reach: an entry with no usable full_name is unstated, not skipped" {
    const declared = [_][]const u8{"acme/payments"};
    // Skipping an unreadable entry would let a malformed response pass whenever
    // the readable entries happened to match.
    try testing.expectEqual(Verdict.unstated, try verdictOf(&declared, "{\"repositories\":[{\"id\":7}]}"));
    try testing.expectEqual(Verdict.unstated, try verdictOf(&declared, "{\"repositories\":[\"acme/payments\"]}"));
}

/// Parse a response body and run `verifyPermissions` against it.
fn permissionsVerdictOf(access: ctx.RepositoryAccess, body: []const u8) !Verdict {
    var parsed = try std.json.parseFromSlice(std.json.Value, testing.allocator, body, .{});
    defer parsed.deinit();
    return verifyPermissions(access, parsed.value.object);
}

test "test_reach_verifies_permissions" {
    // The requested set at each access level is exact.
    try testing.expectEqual(Verdict.exact, try permissionsVerdictOf(.read, "{\"permissions\":{\"contents\":\"read\"}}"));
    try testing.expectEqual(Verdict.exact, try permissionsVerdictOf(.write, "{\"permissions\":{\"contents\":\"write\",\"pull_requests\":\"write\"}}"));

    // A requested permission missing or at the wrong level refuses — a token
    // NARROWER than asked fails here with a local explanation, and a read
    // binding answered with write contents is the overreach this check exists for.
    try testing.expectEqual(Verdict.mismatched, try permissionsVerdictOf(.write, "{\"permissions\":{\"contents\":\"write\"}}"));
    try testing.expectEqual(Verdict.mismatched, try permissionsVerdictOf(.write, "{\"permissions\":{\"contents\":\"read\",\"pull_requests\":\"write\"}}"));
    try testing.expectEqual(Verdict.mismatched, try permissionsVerdictOf(.read, "{\"permissions\":{\"contents\":\"write\"}}"));

    // An UNREQUESTED write/admin grant refuses; ambient read grants pass —
    // GitHub attaches metadata:read to every installation token.
    try testing.expectEqual(Verdict.mismatched, try permissionsVerdictOf(.read, "{\"permissions\":{\"contents\":\"read\",\"workflows\":\"write\"}}"));
    try testing.expectEqual(Verdict.mismatched, try permissionsVerdictOf(.write, "{\"permissions\":{\"contents\":\"write\",\"pull_requests\":\"write\",\"administration\":\"admin\"}}"));
    try testing.expectEqual(Verdict.exact, try permissionsVerdictOf(.read, "{\"permissions\":{\"contents\":\"read\",\"metadata\":\"read\"}}"));

    // Absent or malformed permissions are unstated, refused — never read as
    // "the installation's full set".
    try testing.expectEqual(Verdict.unstated, try permissionsVerdictOf(.read, "{\"token\":\"ghs_x\"}"));
    try testing.expectEqual(Verdict.unstated, try permissionsVerdictOf(.read, "{\"permissions\":\"all\"}"));
    try testing.expectEqual(Verdict.unstated, try permissionsVerdictOf(.read, "{\"permissions\":{\"contents\":7}}"));
}
