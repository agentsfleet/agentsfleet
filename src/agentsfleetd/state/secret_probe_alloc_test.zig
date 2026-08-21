//! Allocation-failure proof for `secret_probe.probeSelfManagedSecret`.
//!
//! The probe is on a leak-capable path: `tenant_provider` resolves through it,
//! and the runtime resolve runs from the fleet runtime rather than from a
//! request arena, so a rung that never runs is memory the daemon does not get
//! back. `scripts/classify_rung_callers.py` reports this file `repeating`.
//!
//! Four owned values are built one `alloc.dupe` at a time behind three rungs,
//! and one of those rungs is not ordinary cleanup: the `api_key` rung
//! `secureZero`s the key before freeing it. A rung that never runs therefore
//! leaves a plaintext credential sitting in a freed heap block, which is a
//! disclosure shape rather than a leak shape. "The ladder looks right" is not a
//! standard this function can be held to.
//!
//! **Fixture hazards this file is built around.**
//!
//! 1. `resolvePrimaryWorkspace` takes the tenant's EARLIEST workspace
//!    (`ORDER BY created_at ASC, id ASC`), and the shared fixture seeds every
//!    workspace at `created_at = 0`. On the shared tenant the tie breaks on id,
//!    so a sibling suite's row could win and the probe would read a workspace
//!    this file never seeded. It therefore owns its tenant outright.
//! 2. `base_url` is only an allocation site when the secret carries one AND the
//!    provider is the openai-compatible id — a named provider with a base_url is
//!    rejected before any dupe. The seed is openai-compatible with an https URL,
//!    and the wrapper asserts the probe came back with all four values, so a
//!    fixture that stopped reaching the fourth site fails loudly instead of
//!    passing quietly.
//! 3. The probe only READS, so no run of it commits and none needs a per-run
//!    reset — unlike the claim path in `repair_verifications`.
//!
//! `base_url_guard.validate` is purely lexical (no name resolution), so the
//! seeded URL keeps this hermetic.

const std = @import("std");
const pg = @import("pg");

const secret_probe = @import("secret_probe.zig");
const base = @import("../db/test_fixtures.zig");
const crypto_primitives = @import("../secrets/crypto_primitives.zig");

const ALLOC = std.testing.allocator;

/// This file's own tenant and workspace — see fixture hazard 1.
const TENANT_ALLOC_PROBE = "0195b4ba-8d3a-7f13-8abc-ab3000000001";
const WS_ALLOC_PROBE = "0195b4ba-8d3a-7f13-8abc-ab3000000002";
const TENANT_NAME = "secret-probe-alloc-proof";

const SECRET_REF = "tenant:alloc-proof-endpoint";
const PROVIDER_COMPATIBLE = secret_probe.OPENAI_COMPATIBLE_PROVIDER;
const BASE_URL = "https://gateway.alloc-proof.invalid/v1";
const API_KEY = "sk-alloc-proof-not-a-real-key";
const MODEL = "accounts/alloc-proof/models/probe";

const FIELD_PROVIDER = "provider";
const FIELD_API_KEY = "api_key";
const FIELD_MODEL = "model";
const FIELD_BASE_URL = "base_url";

fn seed(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    try base.seedTenantById(conn, TENANT_ALLOC_PROBE, TENANT_NAME);
    try base.seedWorkspaceWithTenant(conn, WS_ALLOC_PROBE, TENANT_ALLOC_PROBE);

    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, FIELD_PROVIDER, .{ .string = PROVIDER_COMPATIBLE });
    try obj.put(alloc, FIELD_API_KEY, .{ .string = API_KEY });
    try obj.put(alloc, FIELD_MODEL, .{ .string = MODEL });
    try obj.put(alloc, FIELD_BASE_URL, .{ .string = BASE_URL });
    try base.storeVaultJson(alloc, conn, WS_ALLOC_PROBE, SECRET_REF, .{ .object = obj });
}

fn cleanup(conn: *pg.Conn) void {
    _ = conn.exec(
        "DELETE FROM vault.secrets WHERE workspace_id = $1::uuid",
        .{WS_ALLOC_PROBE},
    ) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    base.teardownWorkspace(conn, WS_ALLOC_PROBE);
    base.teardownTenantById(conn, TENANT_ALLOC_PROBE);
}

// ── Wrapper ───────────────────────────────────────────────────────────────
// `checkAllAllocationFailures` requires `fn(Allocator, ...) !void`. The wrapper
// frees on the SUCCESS path only: on an induced failure the probe owns the
// unwind, which is the whole point.

fn probeUnderAllocator(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    var probed = try secret_probe.probeSelfManagedSecret(
        alloc,
        conn,
        TENANT_ALLOC_PROBE,
        SECRET_REF,
    );
    defer probed.deinit(alloc);

    // Fixture hazard 2: if any of these came back empty or absent, its dupe was
    // never an allocation site and a green run below would prove nothing.
    if (probed.provider.len == 0) return error.FixtureProviderEmpty;
    if (probed.api_key.len == 0) return error.FixtureApiKeyEmpty;
    if (probed.model.len == 0) return error.FixtureModelEmpty;
    _ = probed.base_url orelse return error.FixtureBaseUrlNull;
}

// ── Proof ─────────────────────────────────────────────────────────────────

test "every allocation site in the self-managed secret probe unwinds without leaking" {
    crypto_primitives.setTestKek();
    const handle = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seed(ALLOC, handle.conn);
    defer cleanup(handle.conn);

    try std.testing.checkAllAllocationFailures(
        ALLOC,
        probeUnderAllocator,
        .{handle.conn},
    );
}
