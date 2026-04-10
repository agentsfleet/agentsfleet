//! Static compile-time network allowlist for the executor sidecar (M16_003 §3.2).
//!
//! Phase 1: bare-metal hosts set EXECUTOR_NETWORK_POLICY=registry_allowlist to
//! permit agent dependency installs (npm, pip, cargo, go get) against public
//! package registries. In Phase 1, --share-net is passed to bwrap and the full
//! host network is accessible; the REGISTRY_ALLOWLIST entries are logged for
//! observability only — TCP-layer restriction to these hosts is Phase 2 (nftables).
//!
//! Phase 2 (out of scope): nftables egress rules restrict traffic to REGISTRY_ALLOWLIST.

/// Public package registries permitted under the `registry_allowlist` policy.
/// This is a compile-time constant — no per-run override path exists.
pub const REGISTRY_ALLOWLIST = [_][]const u8{
    "registry.npmjs.org",
    "pypi.org",
    "files.pythonhosted.org",
    "static.crates.io",
    "crates.io",
    "index.crates.io",
    "proxy.golang.org",
    "sum.golang.org",
};

/// M2_001: Zombie skill-specific domain allowlists.
/// Keyed by skill name from ZombieConfig.skills[].
/// The executor merges these with the Zombie's config.network.allow list.
pub const ZOMBIE_SKILL_ALLOWLIST = struct {
    pub const agentmail = [_][]const u8{
        "api.agentmail.dev",
    };
};

// ── Tests ─────────────────────────────────────────────────────────────────────

const std = @import("std");

// T1 — Happy path

test "REGISTRY_ALLOWLIST is non-empty" {
    try std.testing.expect(REGISTRY_ALLOWLIST.len > 0);
}

// T2 / T10 — All 8 expected hosts are present and correct

test "REGISTRY_ALLOWLIST contains all 8 expected package registry hosts" {
    // Spec §3.1 lists exactly these hosts. A missing entry means agent dependency
    // installs for the corresponding ecosystem will fail silently on bare-metal.
    const expected = [_][]const u8{
        "registry.npmjs.org",
        "pypi.org",
        "files.pythonhosted.org",
        "static.crates.io",
        "crates.io",
        "index.crates.io",
        "proxy.golang.org",
        "sum.golang.org",
    };
    try std.testing.expectEqual(@as(usize, expected.len), REGISTRY_ALLOWLIST.len);
    for (expected) |want| {
        var found = false;
        for (REGISTRY_ALLOWLIST) |have| {
            if (std.mem.eql(u8, have, want)) {
                found = true;
                break;
            }
        }
        if (!found) {
            std.debug.print("\nREGISTRY_ALLOWLIST missing host: {s}\n", .{want});
        }
        try std.testing.expect(found);
    }
}

// T2 — No duplicate entries

test "REGISTRY_ALLOWLIST has no duplicate hostnames" {
    for (REGISTRY_ALLOWLIST, 0..) |a, i| {
        for (REGISTRY_ALLOWLIST[i + 1 ..]) |b| {
            if (std.mem.eql(u8, a, b)) {
                std.debug.print("\nREGISTRY_ALLOWLIST duplicate entry: {s}\n", .{a});
                try std.testing.expect(false);
            }
        }
    }
}

// T10 — Every entry is a non-empty string with no whitespace

test "REGISTRY_ALLOWLIST entries are non-empty and contain no whitespace" {
    for (REGISTRY_ALLOWLIST) |host| {
        try std.testing.expect(host.len > 0);
        for (host) |ch| {
            try std.testing.expect(!std.ascii.isWhitespace(ch));
        }
    }
}

// T7 — Exact count pinned — catches accidental addition or removal

test "REGISTRY_ALLOWLIST length is exactly 8 (spec §3.1 pinned count)" {
    // If this fails the spec §3.1 list changed — update this test intentionally.
    try std.testing.expectEqual(@as(usize, 8), REGISTRY_ALLOWLIST.len);
}
