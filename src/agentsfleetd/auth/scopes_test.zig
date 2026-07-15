//! Tests for the scope catalog. FLL-exempt.

const std = @import("std");
const testing = std.testing;
const scopes = @import("scopes.zig");
const Scope = scopes.Scope;

// ── Dimension 1.1 — catalog covers every enumerated gate ───────────────────

test "test_scope_catalog_covers_every_enumerated_gate" {
    // The capabilities surfaced by the Jun-29 gate sweep (route_table.zig +
    // workspace_guards.zig). Every one must have a wire string that round-trips,
    // proving the catalog is a superset of the enumeration checklist (Invariant 5).
    const enumerated = [_][]const u8{
        // Platform plane (former platform_admin routes).
        "platform-key:read", "platform-key:admin", "model:read",    "model:admin",
        "runner:enroll",     "runner:read",        "runner:write",  "stream:read",
        // Tenant plane (former bearer + operator-role + enforce routes).
        "fleet:read",        "fleet:write",        "fleet:admin",   "secret:read",
        "secret:write",      "apikey:read",        "apikey:write",  "apikey:admin",
        "fleetkey:read",     "fleetkey:write",     "grant:read",    "grant:write",
        "connector:read",    "connector:write",    "billing:read",  "approval:read",
        "approval:resolve",  "workspace:admin",    "library:write", "platform-library:write",
        // Runner credential.
        "runner:self",
        // Cross-tenant override (single scope covering read + write).
              "workspace:any",
    };
    for (enumerated) |wire_str| {
        const set = scopes.parseClaim(wire_str);
        // A known wire string parses to a non-empty held set.
        try testing.expect(set.count() >= 1);
    }
    // And every wire string round-trips through the enum.
    inline for (@typeInfo(Scope).@"enum".fields) |f| {
        const s: Scope = @enumFromInt(f.value);
        const round = scopes.parseClaim(s.wire());
        try testing.expect(round.contains(s));
    }
}

// ── Dimension 1.2 — hierarchy subsumes lower, stored as data ───────────────

test "test_scope_hierarchy_subsumes_lower" {
    // Holding fleet:admin satisfies fleet:write AND fleet:read (downward closure
    // at parse time), proving the map is data — not string-prefix inference.
    const held = scopes.parseClaim("fleet:admin");
    try testing.expect(held.contains(.fleet_admin));
    try testing.expect(held.contains(.fleet_write));
    try testing.expect(held.contains(.fleet_read));

    // apikey:write satisfies apikey:read but NOT apikey:admin (upward never).
    const w = scopes.parseClaim("apikey:write");
    try testing.expect(w.contains(.apikey_write));
    try testing.expect(w.contains(.apikey_read));
    try testing.expect(!w.contains(.apikey_admin));

    // platform-library:write and library:write are independent (no hierarchy).
    const ptw = scopes.parseClaim("platform-library:write");
    try testing.expect(ptw.contains(.platform_library_write));
    try testing.expect(!ptw.contains(.library_write)); // independent, not laddered

    // A discrete verb subsumes nothing.
    const enroll = scopes.parseClaim("runner:enroll");
    try testing.expectEqual(@as(usize, 1), enroll.count());
}

// ── Dimension 1.3 surface — parse populates / fails closed ─────────────────

test "test_principal_scopes_populated_from_claim" {
    // Space-delimited multi-scope claim → exactly those (+ their closure).
    const held = scopes.parseClaim("fleet:read secret:write");
    try testing.expect(held.contains(.fleet_read));
    try testing.expect(held.contains(.secret_write));
    try testing.expect(held.contains(.secret_read)); // write ⊇ read
    try testing.expect(!held.contains(.fleet_write));

    // Absent claim → empty set (every capability gate then fails closed).
    try testing.expectEqual(@as(usize, 0), scopes.parseClaim("").count());

    // Unknown / typo strings grant nothing (deny by absence).
    try testing.expectEqual(@as(usize, 0), scopes.parseClaim("fleet:destroy wat").count());
    const mixed = scopes.parseClaim("bogus fleet:read alsobogus");
    try testing.expectEqual(@as(usize, 1), mixed.count());
    try testing.expect(mixed.contains(.fleet_read));
}

// ── Dimension 2.1 surface — any-of semantics ───────────────────────────────

test "test_require_scope_any_of_with_hierarchy" {
    const route_any_of = [_]Scope{ .fleet_read, .fleet_write, .fleet_admin };

    // Holder of fleet:admin satisfies the any-of (hierarchy-expanded).
    try testing.expect(scopes.satisfiesAny(scopes.parseClaim("fleet:admin"), &route_any_of));
    // Holder of only fleet:read satisfies a GET's any-of.
    try testing.expect(scopes.satisfiesAny(scopes.parseClaim("fleet:read"), &route_any_of));
    // Empty held set is denied against a non-empty requirement (fail closed).
    try testing.expect(!scopes.satisfiesAny(scopes.parseClaim(""), &route_any_of));
    // A DELETE demands :admin specifically — fleet:write does not satisfy it.
    try testing.expect(!scopes.satisfiesAny(scopes.parseClaim("fleet:write"), &[_]Scope{.fleet_admin}));
    // Empty requirement = authenticated-only route → always allowed.
    try testing.expect(scopes.satisfiesAny(scopes.parseClaim(""), &[_]Scope{}));
}

// ── Dimension 1.4 — default grants provision explicit scopes; never enforced ──

test "test_default_grants_provision_and_are_not_enforced" {
    // .tenant = the signup-owner + `agt_t` api-key grant: every tenant
    // capability, NO platform / cross-tenant scope.
    const owner = scopes.defaultScopes(.tenant);
    try testing.expect(owner.contains(.fleet_admin));
    try testing.expect(owner.contains(.fleet_read)); // closure
    try testing.expect(owner.contains(.secret_write));
    try testing.expect(owner.contains(.workspace_admin));
    try testing.expect(owner.contains(.library_write));
    try testing.expect(!owner.contains(.platform_library_write)); // tenant tier only
    try testing.expect(!owner.contains(.runner_enroll));
    try testing.expect(!owner.contains(.workspace_any));
    try testing.expect(!owner.contains(.model_admin));

    // The IdP writeback string carries the minimal (pre-closure) scopes and NO
    // platform/cross-tenant scope — the signup owner never gets `workspace:any`.
    const wire = scopes.defaultClaim(.tenant);
    try testing.expect(std.mem.indexOf(u8, wire, "fleet:admin") != null);
    try testing.expect(std.mem.indexOf(u8, wire, "workspace:admin") != null);
    try testing.expect(std.mem.indexOf(u8, wire, "workspace:any") == null);
    try testing.expect(std.mem.indexOf(u8, wire, "runner:enroll") == null);

    // .runner = self-plane only.
    const r = scopes.defaultScopes(.runner);
    try testing.expectEqual(@as(usize, 1), r.count());
    try testing.expect(r.contains(.runner_self));

    // Gate signature rejects a DefaultGrant at compile time: satisfiesAny takes
    // []const Scope, never DefaultGrant. The following would not compile —
    //   scopes.satisfiesAny(held, &[_]scopes.DefaultGrant{.tenant});
    // (documented here; enforced by the type system, Invariant 10).
}
