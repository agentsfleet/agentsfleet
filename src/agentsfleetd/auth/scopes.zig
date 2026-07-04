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
//! `DefaultGrant` maps a credential source (`tenant` / `runner`) to the explicit
//! scope set provisioned onto its principal at construction — PROVISIONING ONLY,
//! no gate ever checks a grant (Invariant 10); gates take `Scope` values.
//! Operator/collaborator scope sets are provisioned manually at the IdP
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
    secret_read,
    secret_write,
    apikey_read,
    apikey_write,
    apikey_admin,
    fleetkey_read,
    fleetkey_write,
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

/// Credential sources that receive a default scope grant at principal
/// construction. Keyed by *where the principal comes from* (the real axis), not
/// a role name — the provisioning twin of `route_scopes.zig`. `defaultScopes` /
/// `defaultClaim` are NEVER consulted at a gate (gates take `Scope`, not a
/// grant — Invariant 10). Operator/collaborator grants are provisioned manually
/// at the IdP (see docs/AUTH.md); nothing in code expands them.
pub const DefaultGrant = enum { tenant, runner };

/// The minimal scopes provisioned to `src` (before hierarchy closure).
fn grantMembers(src: DefaultGrant) []const Scope {
    return switch (src) {
        // Clerk signup owner + `agt_t` tenant api-key: every tenant capability,
        // NO platform or cross-tenant scope (preserves "an admin api-key cannot
        // enroll a runner").
        .tenant => &.{
            .fleet_admin,
            .secret_write,
            .apikey_admin,
            .fleetkey_write,
            .grant_write,
            .connector_write,
            .billing_read,
            .approval_resolve,
            .workspace_admin,
            .library_write,
        },
        // Host-resident runner credential — self-plane only.
        .runner => &.{.runner_self},
    };
}

/// The hierarchy-expanded scope set provisioned to `src` — written onto the
/// principal at construction (`tenant_api_key`, `runner_bearer`).
pub fn defaultScopes(comptime src: DefaultGrant) Set {
    // Comptime-pinned: the grant + hierarchy are statically known, so each
    // credential source resolves to a constant Set inlined at the call site —
    // no per-request closure walk, and the set can't be silently widened.
    return comptime blk: {
        var set = Set.initEmpty();
        for (grantMembers(src)) |s| insertWithClosure(&set, s);
        break :blk set;
    };
}

/// The space-delimited wire string provisioned to `src` — the exact value
/// written into the IdP's `public_metadata.scopes` and read back into the
/// `scopes` claim. Comptime-built; the parser expands the hierarchy on read, so
/// lower rungs are omitted here.
pub fn defaultClaim(comptime src: DefaultGrant) []const u8 {
    return comptime blk: {
        var s: []const u8 = "";
        for (grantMembers(src), 0..) |scope, i| {
            s = s ++ (if (i == 0) "" else " ") ++ scope.wire();
        }
        break :blk s;
    };
}

// ── Wire strings (RULE UFS — the claim values shared verbatim with Clerk) ────────────────

const ScopeWire = struct { scope: Scope, str: []const u8 };

const WIRE = [_]ScopeWire{
    .{ .scope = .fleet_read, .str = "fleet:read" },
    .{ .scope = .fleet_write, .str = "fleet:write" },
    .{ .scope = .fleet_admin, .str = "fleet:admin" },
    .{ .scope = .secret_read, .str = "secret:read" },
    .{ .scope = .secret_write, .str = "secret:write" },
    .{ .scope = .apikey_read, .str = "apikey:read" },
    .{ .scope = .apikey_write, .str = "apikey:write" },
    .{ .scope = .apikey_admin, .str = "apikey:admin" },
    .{ .scope = .fleetkey_read, .str = "fleetkey:read" },
    .{ .scope = .fleetkey_write, .str = "fleetkey:write" },
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
    .{ .scope = .secret_write, .includes = &.{.secret_read} },
    .{ .scope = .apikey_admin, .includes = &.{ .apikey_write, .apikey_read } },
    .{ .scope = .apikey_write, .includes = &.{.apikey_read} },
    .{ .scope = .fleetkey_write, .includes = &.{.fleetkey_read} },
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

test {
    _ = @import("scopes_test.zig");
}
