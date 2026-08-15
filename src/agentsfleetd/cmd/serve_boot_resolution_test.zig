//! Boot-time resolution seams split out of `serve.zig`: the R2 client, the
//! webhook/backend secrets, and the process deadline scheduler.
//!
//! None of the three had an executed line. Each one decides whether a
//! capability exists for the process lifetime — storage silently disabled by a
//! missing credential, a webhook middleware failing closed on an absent
//! secret — so the resolution rules are pinned, not assumed.

const std = @import("std");
const common = @import("common");

const serve_r2 = @import("serve_r2.zig");
const serve_secrets = @import("serve_secrets.zig");
const serve_deadline = @import("serve_deadline.zig");
const env_resolve = @import("../config/env_resolve.zig");
const clerk_backend = @import("../auth/clerk_backend.zig");

const ALLOC = std.testing.allocator;

// ── serve_r2 ────────────────────────────────────────────────────────────────

const R2_FULL = [_][2][]const u8{
    .{ env_resolve.R2_ACCOUNT_ID_ENV, "acct-1" },
    .{ env_resolve.R2_ACCESS_KEY_ID_ENV, "ak-1" },
    .{ env_resolve.R2_SECRET_ACCESS_KEY_ENV, "sk-1" },
    .{ env_resolve.R2_BUCKET_ENV, "bundles" },
};

test "r2 resolves a client only when all four credentials are present" {
    var map = try common.env.fromPairs(ALLOC, &R2_FULL);
    defer map.deinit();

    var r2 = (try serve_r2.resolve(&map, ALLOC, std.testing.io)) orelse return error.TestUnexpectedResult;
    defer r2.deinit();
    try std.testing.expectEqualStrings("bundles", r2.bucket);
}

test "r2 stays disabled when any one credential is absent" {
    // Drop each of the four in turn: storage must disable, never half-init.
    for (0..R2_FULL.len) |drop| {
        var pairs: std.ArrayList([2][]const u8) = .empty;
        defer pairs.deinit(ALLOC);
        for (R2_FULL, 0..) |kv, i| {
            if (i != drop) try pairs.append(ALLOC, kv);
        }
        var map = try common.env.fromPairs(ALLOC, pairs.items);
        defer map.deinit();

        try std.testing.expect((try serve_r2.resolve(&map, ALLOC, std.testing.io)) == null);
    }
}

test "r2 treats an empty credential as unset, not as a value" {
    var map = try common.env.fromPairs(ALLOC, &.{
        .{ env_resolve.R2_ACCOUNT_ID_ENV, "" }, // set-but-empty
        .{ env_resolve.R2_ACCESS_KEY_ID_ENV, "ak-1" },
        .{ env_resolve.R2_SECRET_ACCESS_KEY_ENV, "sk-1" },
        .{ env_resolve.R2_BUCKET_ENV, "bundles" },
    });
    defer map.deinit();

    try std.testing.expect((try serve_r2.resolve(&map, ALLOC, std.testing.io)) == null);
}

// ── serve_secrets ───────────────────────────────────────────────────────────

test "secrets resolve each present value and null for the unset ones" {
    var map = try common.env.fromPairs(ALLOC, &.{
        .{ env_resolve.CLERK_WEBHOOK_SECRET_ENV, "whsec_probe" },
        .{ clerk_backend.SECRET_ENV_VAR, "sk_clerk_probe" },
        // APPROVAL_SIGNING_SECRET deliberately unset → consumer fails closed.
    });
    defer map.deinit();

    var secrets = try serve_secrets.resolve(&map, ALLOC);
    defer secrets.deinit();

    try std.testing.expectEqualStrings("whsec_probe", secrets.clerk_webhook_secret.?);
    try std.testing.expectEqualStrings("sk_clerk_probe", secrets.clerk_secret_key.?);
    try std.testing.expect(secrets.approval_signing_secret == null);
}

test "secrets from an empty environment are all null, and deinit is safe" {
    var map = try common.env.fromPairs(ALLOC, &.{});
    defer map.deinit();

    var secrets = try serve_secrets.resolve(&map, ALLOC);
    defer secrets.deinit();

    try std.testing.expect(secrets.clerk_webhook_secret == null);
    try std.testing.expect(secrets.approval_signing_secret == null);
    try std.testing.expect(secrets.clerk_secret_key == null);
}

// ── serve_deadline ──────────────────────────────────────────────────────────

test "the deadline scheduler starts, hands out a live handle, and tears down" {
    var owned: serve_deadline.Owned = .{};
    defer owned.deinit();

    const scheduler = owned.start(ALLOC);
    // The borrowed handle is the one inside `owned` — the address every
    // network owner arms against must be process-stable.
    try std.testing.expectEqual(&owned.scheduler.?, scheduler);
}
