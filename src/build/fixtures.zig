//! fixtures.zig — wires test-fixture files onto a test module so the
//! `*_test.zig` sources can `@embedFile` them by name.
//!
//! Fixtures live under `tests/fixtures/` (outside every source module's root
//! directory). `@embedFile` resolves a relative path against the importing
//! file's module root and refuses to read a file the root does not contain, so
//! a fixture at the repo root is unreachable by path from `src/agentsfleetd/**`
//! or `src/runner/**`. Registering it as a named anonymous import makes
//! `@embedFile("<name>")` resolve it regardless of where the source file sits.
//!
//! Mirrors the pg.zig / s3.zig helper split — keeps build.zig and
//! build_runner.zig free of per-fixture wiring.

const std = @import("std");

const Fixture = struct { name: []const u8, path: []const u8 };

/// agentsfleetd (`src/agentsfleetd/**`) `*_test.zig` @embedFile fixtures.
const DAEMON: []const Fixture = &.{
    .{ .name = "github_run_failure.json", .path = "tests/fixtures/webhooks/github_run_failure.json" },
    .{ .name = "github_run_success.json", .path = "tests/fixtures/webhooks/github_run_success.json" },
    .{ .name = "sample_with_folders.tar.gz", .path = "tests/fixtures/fleetbundle/sample_with_folders.tar.gz" },
};

/// agentsfleet-runner (`src/runner/**`) `*_test.zig` @embedFile fixtures.
const RUNNER: []const Fixture = &.{
    .{ .name = "help.txt", .path = "tests/fixtures/runner/help.txt" },
    // Real security-reviewer bundle — bundle_extract_test.zig builds a tar from
    // these to exercise nested-folder (checklists/) support-file materialization.
    .{ .name = "security-reviewer-SKILL.md", .path = "tests/fixtures/fleetbundle/security-reviewer/SKILL.md" },
    .{ .name = "security-reviewer-TRIGGER.md", .path = "tests/fixtures/fleetbundle/security-reviewer/TRIGGER.md" },
    .{ .name = "security-reviewer-owasp.md", .path = "tests/fixtures/fleetbundle/security-reviewer/checklists/owasp.md" },
    .{ .name = "06_rule_drop_dns_udp.mnl.txt", .path = "tests/fixtures/runner/network/captured/06_rule_drop_dns_udp.mnl.txt" },
    .{ .name = "07_rule_drop_dns_tcp.mnl.txt", .path = "tests/fixtures/runner/network/captured/07_rule_drop_dns_tcp.mnl.txt" },
    .{ .name = "08_rule_allow_set.mnl.txt", .path = "tests/fixtures/runner/network/captured/08_rule_allow_set.mnl.txt" },
    .{ .name = "09_rule_ct_return.mnl.txt", .path = "tests/fixtures/runner/network/captured/09_rule_ct_return.mnl.txt" },
    .{ .name = "10_rule_masquerade.mnl.txt", .path = "tests/fixtures/runner/network/captured/10_rule_masquerade.mnl.txt" },
};

/// Register the agentsfleetd test fixtures on `module` (the `tests` root module).
pub fn addDaemon(b: *std.Build, module: *std.Build.Module) void {
    addAll(b, module, DAEMON);
}

/// Register the agentsfleet-runner test fixtures on `module` (the `runner_tests` root module).
pub fn addRunner(b: *std.Build, module: *std.Build.Module) void {
    addAll(b, module, RUNNER);
}

fn addAll(b: *std.Build, module: *std.Build.Module, fixtures: []const Fixture) void {
    for (fixtures) |f| {
        module.addAnonymousImport(f.name, .{ .root_source_file = b.path(f.path) });
    }
}
