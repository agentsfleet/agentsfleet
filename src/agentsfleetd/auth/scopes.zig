//! Scope catalog — the `resource:action` capability vocabulary.
//!
//! One explicit scope per capability replaces the old `AuthRole` ladder +
//! `platform_admin` bool. The hierarchy (`read < write < admin`) is stored as
//! DATA (`HIERARCHY`), never inferred from the string — Sentry's
//! `SENTRY_SCOPE_HIERARCHY_MAPPING` shape (`conf/server.py`). A held scope is
//! expanded to its downward closure at parse time, so the request-time gate is
//! a trivial `Set.contains` (bun's declarative-table-not-vtable instinct).
//!
//! Wire strings (the JWT `scopes` claim values, shared verbatim with Clerk —
//! RULE UFS) live in `WIRE`; Zig enum tags cannot carry the `:` separator, so
//! the tag (`fleet_read`) and the wire value (`fleet:read`) are paired in the
//! comptime table and validated total over the enum.
//!
//! Two scope sets are still named in code, and they are named for opposite
//! reasons. `SIGNUP_OWNER_CLAIM` is WRITTEN to the identity provider once by
//! the `user.created` writeback and never read back from here — the provider
//! owns the value from that instant. `RUNNER_SCOPES` is READ at principal
//! construction, and only because a runner credential names a machine that has
//! no identity at the provider to ask. Every credential that names a person
//! resolves its capabilities from the provider per request.
//!
//! Neither is a gate: no gate checks a grant (Invariant 10); gates take `Scope`
//! values. Operator/collaborator scope sets are provisioned manually at the IdP
//! (documented in docs/AUTH.md), so they are documentation, not code.
//!
//! Portability: like every `src/auth/**` file this imports only `std`.

/// Every capability gate maps to exactly one variant. The wire string (claim
/// value) is `wire(self)`, NOT `@tagName` — tags cannot hold `:`.
pub const Scope = enum {
    // ── Laddered resources (read < write < admin), hierarchy in HIERARCHY ──
    fleet_read,
    fleet_write,
    fleet_admin,
    schedule_read,
    schedule_write,
    secret_read,
    secret_write,
    apikey_read,
    apikey_write,
    apikey_admin,
    grant_read,
    grant_write,
    connector_read,
    connector_write,
    model_read,
    model_admin,
    platform_key_read,
    platform_key_admin,
    // Operator plane over EXISTING runners: read = list/events, write = cordon/patch.
    runner_read,
    runner_write,
    // ── Single-action reads (no write rung) ───────────────────────────────
    stream_read,
    approval_read,
    // ── Discrete verbs (a distinct action, not generic CRUD) ──────────────
    runner_enroll, // create a trusted host (mint agt_r) — uniquely dangerous, isolated
    approval_resolve, // decide an approval gate (approve/deny)
    billing_read,
    workspace_admin,
    // Fleet library (M103 consumes these): write = tenant-tier onboarding
    // (held by a workspace owner), platform_library_write = platform-tier
    // onboarding (held by a platform operator). Independent — no hierarchy.
    library_write,
    platform_library_write,
    // ── Runner credential (machine identity — minted onto the agt_r token) ─
    runner_self,
    // ── Cross-tenant override (held by almost no one; every use audited) ──
    // One scope covers read AND write across tenants — a holder can view and act
    // on any tenant's workspace. The ownership check bypasses the tenant-id match
    // for this principal and emits an audit record on every crossing.
    workspace_any,

    /// The JWT claim value for this scope. Verbatim-matched in Clerk config.
    pub fn wire(self: Scope) []const u8 {
        inline for (WIRE) |pair| {
            if (pair.scope == self) return pair.str;
        }
        unreachable; // WIRE is total over Scope (asserted at comptime below).
    }
};

/// A principal's held capabilities. A bitset — no allocation, no lifetime.
/// Always stores the downward closure of what was granted (see `parseClaim`),
/// so `satisfies` is a single membership test.
pub const Set = std.EnumSet(Scope);

/// The tenant owner's grant, written to the provider once at signup by the
/// `user.created` writeback. NO platform or cross-tenant scope — that is what
/// preserves "an admin cannot enroll a runner".
///
/// There is deliberately no machine twin of this list. An `agt_t` key used to
/// carry its own compiled-in grant — this set minus `approval_resolve` — now
/// retired: a key resolves the capabilities the provider
/// holds for the person named in `created_by`, exactly as an `afc_` credential
/// does. A key is as capable as its creator, no more and no less, so there is
/// no second set here to drift from this one.
const TENANT_OWNER_GRANT = [_]Scope{
    .fleet_admin,
    .schedule_write,
    .secret_write,
    .apikey_admin,
    .grant_write,
    .connector_write,
    .billing_read,
    .workspace_admin,
    .library_write,
    .approval_resolve,
};

/// The runner plane's capability, expanded through the hierarchy.
///
/// This is the one set still decided in code at principal construction, and it
/// is decided here because a runner has no identity at the provider to ask: an
/// `agt_r` credential is host-resident and names a machine, not a person. Every
/// credential that names a person — `afc_`, `agt_t`, and any JSON Web Token —
/// resolves its capabilities from the provider instead.
pub const RUNNER_SCOPES: Set = blk: {
    @setEvalBranchQuota(2_000);
    var set = Set.initEmpty();
    insertWithClosure(&set, .runner_self);
    break :blk set;
};

/// The space-delimited claim seeded into a new owner's `public_metadata.scopes`
/// by the `user.created` writeback — a WRITE, not a read. Once it lands, the
/// provider owns the value and every later request reads it from there, so an
/// operator who edits it wins permanently. Nothing consults this at a gate.
///
/// Comptime-built; the parser expands the hierarchy on read, so lower rungs are
/// omitted here.
pub const SIGNUP_OWNER_CLAIM: []const u8 = blk: {
    var s: []const u8 = "";
    for (TENANT_OWNER_GRANT, 0..) |scope, i| {
        s = s ++ (if (i == 0) "" else " ") ++ scope.wire();
    }
    break :blk s;
};

// ── Wire strings (RULE UFS — the claim values shared verbatim with Clerk) ────────────────

const ScopeWire = struct { scope: Scope, str: []const u8 };

const WIRE = [_]ScopeWire{
    .{ .scope = .fleet_read, .str = "fleet:read" },
    .{ .scope = .fleet_write, .str = "fleet:write" },
    .{ .scope = .fleet_admin, .str = "fleet:admin" },
    .{ .scope = .schedule_read, .str = "schedule:read" },
    .{ .scope = .schedule_write, .str = "schedule:write" },
    .{ .scope = .secret_read, .str = "secret:read" },
    .{ .scope = .secret_write, .str = "secret:write" },
    .{ .scope = .apikey_read, .str = "apikey:read" },
    .{ .scope = .apikey_write, .str = "apikey:write" },
    .{ .scope = .apikey_admin, .str = "apikey:admin" },
    .{ .scope = .grant_read, .str = "grant:read" },
    .{ .scope = .grant_write, .str = "grant:write" },
    .{ .scope = .connector_read, .str = "connector:read" },
    .{ .scope = .connector_write, .str = "connector:write" },
    .{ .scope = .model_read, .str = "model:read" },
    .{ .scope = .model_admin, .str = "model:admin" },
    .{ .scope = .platform_key_read, .str = "platform-key:read" },
    .{ .scope = .platform_key_admin, .str = "platform-key:admin" },
    .{ .scope = .runner_read, .str = "runner:read" },
    .{ .scope = .runner_write, .str = "runner:write" },
    .{ .scope = .stream_read, .str = "stream:read" },
    .{ .scope = .approval_read, .str = "approval:read" },
    .{ .scope = .runner_enroll, .str = "runner:enroll" },
    .{ .scope = .approval_resolve, .str = "approval:resolve" },
    .{ .scope = .billing_read, .str = "billing:read" },
    .{ .scope = .workspace_admin, .str = "workspace:admin" },
    .{ .scope = .library_write, .str = "library:write" },
    .{ .scope = .platform_library_write, .str = "platform-library:write" },
    .{ .scope = .runner_self, .str = "runner:self" },
    .{ .scope = .workspace_any, .str = "workspace:any" },
};

comptime {
    @setEvalBranchQuota(2_000);
    // WIRE is total over Scope and collision-free: every variant has exactly
    // one wire string and no two share one. A missing/dup entry is a build error.
    const n = @typeInfo(Scope).@"enum".fields.len;
    std.debug.assert(WIRE.len == n);
    for (@typeInfo(Scope).@"enum".fields) |f| {
        const s: Scope = @enumFromInt(f.value);
        var seen: usize = 0;
        for (WIRE) |pair| {
            if (pair.scope == s) seen += 1;
        }
        std.debug.assert(seen == 1);
    }
}

// ── Hierarchy as data (Sentry shape; NOT string-prefix inference) ──────────

const Subsumption = struct { scope: Scope, includes: []const Scope };

/// `admin` subsumes `write` and `read`; `write` subsumes `read`. Full transitive
/// closure per ladder so `insertWithClosure` is one non-recursive pass.
const HIERARCHY = [_]Subsumption{
    .{ .scope = .fleet_admin, .includes = &.{ .fleet_write, .fleet_read } },
    .{ .scope = .fleet_write, .includes = &.{.fleet_read} },
    .{ .scope = .schedule_write, .includes = &.{.schedule_read} },
    .{ .scope = .secret_write, .includes = &.{.secret_read} },
    .{ .scope = .apikey_admin, .includes = &.{ .apikey_write, .apikey_read } },
    .{ .scope = .apikey_write, .includes = &.{.apikey_read} },
    .{ .scope = .grant_write, .includes = &.{.grant_read} },
    .{ .scope = .connector_write, .includes = &.{.connector_read} },
    .{ .scope = .model_admin, .includes = &.{.model_read} },
    .{ .scope = .platform_key_admin, .includes = &.{.platform_key_read} },
    .{ .scope = .runner_write, .includes = &.{.runner_read} },
    // Deciding an approval gate implies the ability to view the inbox.
    .{ .scope = .approval_resolve, .includes = &.{.approval_read} },
};

fn insertWithClosure(set: *Set, s: Scope) void {
    set.insert(s);
    for (HIERARCHY) |h| {
        if (h.scope == s) {
            for (h.includes) |sub| set.insert(sub);
            return;
        }
    }
}

// ── Parse + check (the request-time surface) ───────────────────────────────

/// Parse a space-delimited claim string (OAuth `scope` convention; the array
/// form is pre-joined with spaces by `claims.zig`) into a held set. Unknown
/// strings are ignored — they grant nothing (deny by absence, Failure Mode
/// "Unknown scope string"). Each granted scope is expanded to its downward
/// closure so a `fleet:admin` grant satisfies `fleet:read` at the gate.
pub fn parseClaim(raw: []const u8) Set {
    var set = Set.initEmpty();
    var it = std.mem.tokenizeScalar(u8, raw, ' ');
    while (it.next()) |tok| {
        if (parseScope(tok)) |s| insertWithClosure(&set, s);
    }
    return set;
}

fn parseScope(str: []const u8) ?Scope {
    for (WIRE) |pair| {
        if (std.mem.eql(u8, pair.str, str)) return pair.scope;
    }
    return null;
}

/// Any-of: the principal is allowed iff it holds at least one required scope.
/// `held` is already hierarchy-expanded, so this is pure membership. An empty
/// `required` means "no capability scope" (authenticated-only routes) → allow;
/// an empty `held` against a non-empty `required` → deny (fail closed).
pub fn satisfiesAny(held: Set, required: []const Scope) bool {
    if (required.len == 0) return true;
    for (required) |r| {
        if (held.contains(r)) return true;
    }
    return false;
}

const std = @import("std");
const common = @import("common");

/// Host-supplied callback answering "what may this subject do, now?" with the
/// identity provider's space-delimited scope claim — the same string shape the
/// JWT path receives in `verified.scopes`, so all three feed one `parseClaim`.
///
/// Injected rather than called directly for the same reason a row lookup is: it
/// reaches the network, and a middleware's branches must be provable without
/// one. The caller owns the returned slice.
///
/// Declared here rather than on either middleware because both the command-line
/// credential and the tenant api-key paths take the same seam and are wired to
/// the same resolver instance at boot. Two identical declarations would be two
/// places for the signature to drift, and the middleware that imported the
/// other's copy would depend on a sibling it has nothing else to say to.
pub const ScopeFn = *const fn (
    scope_host: *anyopaque,
    alloc: std.mem.Allocator,
    oidc_subject: []const u8,
) anyerror![]const u8;

// docs/AUTH.md's Scope catalogue must list every wire string
// this file defines (found missing: platform-library:write, now added).
// Reads WIRE directly (private to this file) rather than duplicating a
// hand-typed list, so a future scope addition here fails this test the
// moment the doc goes stale instead of drifting silently.
test "every WIRE scope string appears in docs/AUTH.md" {
    const alloc = std.testing.allocator;
    // Tests run from the repo root (zig build sets cwd), so the path is
    // relative to the project root — same convention as
    // fleet_runtime/frontmatter_fixtures_test.zig's fixture reads.
    const doc = try std.Io.Dir.cwd().readFileAlloc(common.globalIo(), "docs/AUTH.md", alloc, .limited(256 * 1024));
    defer alloc.free(doc);
    for (WIRE) |w| {
        if (std.mem.indexOf(u8, doc, w.str) == null) {
            std.debug.print("scope missing from docs/AUTH.md: {s}\n", .{w.str});
            return error.TestUnexpectedResult;
        }
    }
}

test {
    _ = @import("scopes_test.zig");
}
