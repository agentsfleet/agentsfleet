//! Bind-contract tests for sandbox_args — the daemon-owned baseline of
//! read-only host paths, and the operator-assigned additions composed onto it.
//!
//! Split from sandbox_args_edge_test.zig on the 350-line bound (RULE FLL); the
//! edge file keeps the argv-shape and platform-arm tests, this one owns which
//! host paths reach a lease and at what mode.
//!
//! Every test here is platform-INDEPENDENT, composition and argv alike. Which
//! host paths reach a lease, and at what mode, is the security property of
//! this surface — so it is proven on every host rather than only on a
//! configured Linux runner.
//!
//! That was not always true, and the gap was the point. These tests used to
//! gate on a real bubblewrap binary and skip without one: on macOS because the
//! host is not Linux, and in continuous integration because the CI image ships
//! no bubblewrap (the product Dockerfile installs it; the CI image never did).
//! So the tests guarding the bind set ran in NO environment, which is exactly
//! how a missing `/run/systemd/resolve` shipped and broke every lease for a
//! week. `composeSandboxPrefix` takes the bwrap path as an argument, so
//! composition is a pure function of the policy and is tested as one.

const std = @import("std");
const contract = @import("contract");
const common = @import("common");

const sandbox_args = @import("sandbox_args.zig");
const Config = @import("daemon/config.zig");

const LANDLOCK_FULL = contract.protocol.SandboxTier.landlock_full;
const WORKSPACE = "/tmp/fleet-ws-bind";

/// Daemon Config for bind tests; buildArgv reads only the policy fields.
fn cfgWithBinds(extra: []const contract.protocol.ExtraBind) Config {
    return Config{
        .control_plane_url = "http://127.0.0.1:8080",
        .runner_token = "agt_rtest",
        .sandbox_tier = LANDLOCK_FULL,
        .storage_home = "/tmp/agentsfleet-runner",
        .network_policy = .deny_all_egress,
        .worker_count = 1,
        .cp_deadlines = .{},
        .registry_allowlist = &.{},
        .extra_binds = extra,
        .alloc = std.testing.allocator,
    };
}

fn indexOfStr(argv: []const []const u8, needle: []const u8) ?usize {
    for (argv, 0..) |s, i| {
        if (std.mem.eql(u8, s, needle)) return i;
    }
    return null;
}

/// Index of a `<flag> <path> <path>` triple, or null. Matching the FLAG plus
/// both operands is what distinguishes a real bind at a specific mode from the
/// path merely appearing somewhere in the argv.
fn bindTripleIndex(argv: []const []const u8, flag: []const u8, path: []const u8) ?usize {
    if (argv.len < 3) return null;
    for (argv[0 .. argv.len - 2], 0..) |s, i| {
        if (!std.mem.eql(u8, s, flag)) continue;
        if (std.mem.eql(u8, argv[i + 1], path) and std.mem.eql(u8, argv[i + 2], path)) return i;
    }
    return null;
}

test "test_composed_binds_are_additive_and_baseline_first" {
    // The composition layer — platform-independent, so the additive-only
    // guarantee is proven on every host rather than only on a Linux runner
    // where the bwrap arm actually emits flags.
    var buf: [sandbox_args.MAX_BINDS]contract.protocol.ExtraBind = undefined;
    const baseline = sandbox_args.RO_SYSTEM_PATHS;

    // No assignment: exactly the baseline, in order, every entry read-only.
    const none = sandbox_args.composeBinds(&buf, &.{});
    try std.testing.expectEqual(baseline.len, none.len);
    for (baseline, none) |want, got| {
        try std.testing.expectEqualStrings(want, got.path);
        try std.testing.expectEqual(contract.protocol.BindMode.read_only, got.mode);
    }

    // With additions: baseline unchanged and still first, operator appended
    // carrying the mode it was assigned.
    const with = sandbox_args.composeBinds(&buf, &.{
        .{ .path = "/srv/models", .mode = .read_write, .note = "shared model cache" },
        .{ .path = "/srv/fonts" },
    });
    try std.testing.expectEqual(baseline.len + 2, with.len);
    for (baseline, with[0..baseline.len]) |want, got| {
        try std.testing.expectEqualStrings(want, got.path);
        try std.testing.expectEqual(contract.protocol.BindMode.read_only, got.mode);
    }
    try std.testing.expectEqualStrings("/srv/models", with[baseline.len].path);
    try std.testing.expectEqual(contract.protocol.BindMode.read_write, with[baseline.len].mode);
    try std.testing.expectEqualStrings("shared model cache", with[baseline.len].note);
    try std.testing.expectEqual(contract.protocol.BindMode.read_only, with[baseline.len + 1].mode);
}

test "test_composed_binds_cannot_drop_or_remode_a_baseline_path" {
    // The security property: NO assignment shape removes a baseline path or
    // makes one writable. `composeBinds` alone CANNOT establish that — it
    // always writes the baseline first, so asserting the baseline slots are
    // intact is true by construction and proves nothing about the sandbox.
    // bwrap applies binds in argv order and the LAST operation on a target
    // wins, so an appended `/etc` read_write would re-mode the baseline mount
    // that composition left sitting untouched in slot 0.
    //
    // What actually holds the invariant is validation refusing the overlap
    // before composition ever runs. That is asserted here, and the argv-level
    // consequence in `test_operator_list_cannot_remove_a_contract_path`.
    for (sandbox_args.RO_SYSTEM_PATHS) |baseline_path| {
        try std.testing.expect(!contract.protocol.extraBindsValid(&.{
            .{ .path = baseline_path, .mode = .read_write },
        }));
        try std.testing.expect(!contract.protocol.extraBindsValid(&.{
            .{ .path = baseline_path, .mode = .read_only },
        }));
    }

    // Composition still holds its half of the bargain for a VALID list.
    var buf: [sandbox_args.MAX_BINDS]contract.protocol.ExtraBind = undefined;
    const composed = sandbox_args.composeBinds(&buf, &.{.{ .path = "/srv/models" }});
    for (sandbox_args.RO_SYSTEM_PATHS, 0..) |baseline_path, i| {
        try std.testing.expectEqualStrings(baseline_path, composed[i].path);
        try std.testing.expectEqual(contract.protocol.BindMode.read_only, composed[i].mode);
    }

    // An over-long list (unreachable past extraBindsValid) degrades to the
    // baseline alone — fail closed, never a truncated half-applied set.
    var over: [contract.protocol.MAX_EXTRA_BINDS + 1]contract.protocol.ExtraBind = undefined;
    for (&over) |*slot| slot.* = .{ .path = "/srv/models" };
    const clamped = sandbox_args.composeBinds(&buf, &over);
    try std.testing.expectEqual(sandbox_args.RO_SYSTEM_PATHS.len, clamped.len);
}

// ── argv-level: which paths reach a lease, at what mode ─────────────────────
//
// These go through `composeSandboxPrefix`, which takes the bwrap path and the
// child exe as arguments instead of probing the host for them. That is what
// lets them run on EVERY platform. Gating them on a real bubblewrap binary
// skipped them on macOS (not Linux) and in continuous integration (the CI
// image ships no bubblewrap) — so the tests guarding which host paths reach a
// lease executed nowhere, which is exactly how the resolver bind went missing.

const FAKE_BWRAP = "/usr/bin/bwrap";
const FAKE_SELF_EXE = "/opt/agentsfleet/bin/agentsfleet-runner";

/// Writable bind flags. `--bind` is the lease workspace; `--bind-try` is an
/// operator entry assigned `read_write`. Both grant write access to a host
/// path, so a test asking "what can this lease write" must scan for both.
const WRITABLE_FLAGS = [_][]const u8{ "--bind", "--bind-try" };

fn prefixWith(alloc: std.mem.Allocator, extra: []const contract.protocol.ExtraBind) ![]const []const u8 {
    return sandbox_args.composeSandboxPrefix(alloc, FAKE_BWRAP, FAKE_SELF_EXE, cfgWithBinds(extra), WORKSPACE, null);
}

test "test_operator_bind_reaches_the_argv_at_its_mode" {
    const alloc = std.testing.allocator;
    // Dimension 4.1 — each assigned path lands under the flag its own mode
    // names.
    const argv = try prefixWith(alloc, &.{
        .{ .path = "/srv/fonts" },
        .{ .path = "/srv/models", .mode = .read_write, .note = "shared model cache" },
    });
    defer sandbox_args.freeArgv(alloc, argv);

    try std.testing.expect(bindTripleIndex(argv, "--ro-bind-try", "/srv/fonts") != null);
    try std.testing.expect(bindTripleIndex(argv, "--bind-try", "/srv/models") != null);
    // The read-only entry is NOT emitted writable, and vice versa — a mode
    // that leaked across entries would silently widen one of them.
    try std.testing.expect(bindTripleIndex(argv, "--bind-try", "/srv/fonts") == null);
    try std.testing.expect(bindTripleIndex(argv, "--ro-bind-try", "/srv/models") == null);
}

test "test_operator_list_cannot_remove_a_contract_path" {
    const alloc = std.testing.allocator;
    // Dimension 4.2 — the operator list is ADDITIVE. No assignment shape
    // un-binds the resolver and re-creates the incident this milestone came
    // from.
    const argv = try prefixWith(alloc, &.{.{ .path = "/srv/models" }});
    defer sandbox_args.freeArgv(alloc, argv);

    for (sandbox_args.RO_SYSTEM_PATHS) |baseline| {
        try std.testing.expect(bindTripleIndex(argv, "--ro-bind-try", baseline) != null);
    }
    // And the baseline precedes the operator's additions, so the composition
    // order is "daemon first, operator appended" rather than interleaved.
    const last_baseline = bindTripleIndex(argv, "--ro-bind-try", sandbox_args.RO_SYSTEM_PATHS[sandbox_args.RO_SYSTEM_PATHS.len - 1]).?;
    try std.testing.expect(bindTripleIndex(argv, "--ro-bind-try", "/srv/models").? > last_baseline);
}

test "should ro-bind the systemd-resolved stub directory so DNS resolves under any network policy" {
    const alloc = std.testing.allocator;
    // The M167 incident, pinned. /etc/resolv.conf symlinks to
    // /run/systemd/resolve/stub-resolv.conf on a systemd-resolved host; without
    // this ro-bind the symlink dangles inside the sandbox's own (always
    // unshared) mount namespace regardless of --share-net, and every outbound
    // DNS lookup fails HostResolutionFailed.
    const argv = try prefixWith(alloc, &.{});
    defer sandbox_args.freeArgv(alloc, argv);

    const path_i = indexOfStr(argv, "/run/systemd/resolve").?;
    try std.testing.expectEqualStrings("--ro-bind-try", argv[path_i - 1]);
    try std.testing.expectEqualStrings("/run/systemd/resolve", argv[path_i + 1]);
}

test "test_every_contract_path_is_bound_at_its_mode" {
    const alloc = std.testing.allocator;
    // Dimension 3.1 — the contract is not prose: every declared path reaches
    // the argv under the flag its declared mode names. Reading the real
    // constant (not a copy) is what makes a dropped entry fail here; a copy
    // would stay green while the live list rotted.
    const argv = try prefixWith(alloc, &.{});
    defer sandbox_args.freeArgv(alloc, argv);

    const flag = contract.protocol.BindMode.read_only.bwrapFlag();
    for (sandbox_args.RO_SYSTEM_PATHS) |p| {
        try std.testing.expect(bindTripleIndex(argv, flag, p) != null);
    }
}

test "test_workspace_is_the_only_writable_bind" {
    const alloc = std.testing.allocator;
    // Dimension 3.2 / Invariant 3 — with no operator list, the workspace is
    // the sole writable mount. Scanning EVERY writable flag is the point: a
    // version of this test that looked only at `--bind` would miss a
    // `--bind-try` entry widening the boundary.
    const argv = try prefixWith(alloc, &.{});
    defer sandbox_args.freeArgv(alloc, argv);

    var writable: usize = 0;
    for (argv, 0..) |s, i| {
        for (WRITABLE_FLAGS) |flag| {
            if (!std.mem.eql(u8, s, flag)) continue;
            try std.testing.expectEqualStrings(WORKSPACE, argv[i + 1]);
            writable += 1;
        }
    }
    try std.testing.expectEqual(@as(usize, 1), writable);

    // And an operator-assigned read_write entry adds exactly one more, itself
    // — the widening is bounded by what the operator named, nothing else.
    const widened = try prefixWith(alloc, &.{.{ .path = "/srv/models", .mode = .read_write }});
    defer sandbox_args.freeArgv(alloc, widened);

    var writable_paths: usize = 0;
    for (widened) |s| {
        for (WRITABLE_FLAGS) |flag| {
            if (std.mem.eql(u8, s, flag)) writable_paths += 1;
        }
    }
    try std.testing.expectEqual(@as(usize, 2), writable_paths);
    try std.testing.expect(bindTripleIndex(widened, "--bind-try", "/srv/models") != null);
}

test "test_every_writable_floor_path_is_a_tmpfs_in_argv" {
    const alloc = std.testing.allocator;
    // Dimension 1.1 — the writable floor is not prose: every entry reaches the
    // argv as a `--tmpfs` mount, and nothing else does. Reading the real
    // shared list is the point; a copy would stay green while the list rotted.
    const argv = try prefixWith(alloc, &.{});
    defer sandbox_args.freeArgv(alloc, argv);

    var tmpfs_count: usize = 0;
    for (argv, 0..) |s_, i| {
        if (std.mem.eql(u8, s_, "--tmpfs")) {
            tmpfs_count += 1;
            var in_floor = false;
            for (contract.protocol.BASELINE_RW_TMPFS) |p_| {
                if (std.mem.eql(u8, argv[i + 1], p_)) in_floor = true;
            }
            try std.testing.expect(in_floor);
        }
    }
    try std.testing.expectEqual(contract.protocol.BASELINE_RW_TMPFS.len, tmpfs_count);
}

test "test_writable_floor_is_never_operator_bindable" {
    // Dimension 1.4 — a mount the sandbox constructs is not an operator's to
    // re-mode: an extra bind naming a floor path is refused in either mode.
    for (contract.protocol.BASELINE_RW_TMPFS) |p_| {
        try std.testing.expect(!contract.protocol.extraBindsValid(&.{
            .{ .path = p_, .mode = .read_write },
        }));
        try std.testing.expect(!contract.protocol.extraBindsValid(&.{
            .{ .path = p_, .mode = .read_only },
        }));
    }
}

test "test_contract_and_argv_agree_exactly" {
    const alloc = std.testing.allocator;
    // Dimension 3.3 — the agreement is BIDIRECTIONAL. One direction (every
    // entry is bound) catches a dropped path; the other (every bound path has
    // an entry) catches one added to the argv without being declared. Only
    // both together make the contract the single source of truth.
    const argv = try prefixWith(alloc, &.{});
    defer sandbox_args.freeArgv(alloc, argv);

    // → no entry without a bind
    for (sandbox_args.RO_SYSTEM_PATHS) |p| {
        try std.testing.expect(bindTripleIndex(argv, "--ro-bind-try", p) != null);
    }

    // ← no bind without an entry. With an empty operator list every
    // `--ro-bind-try` triple must name a declared contract path.
    var seen: usize = 0;
    for (argv, 0..) |s, i| {
        if (!std.mem.eql(u8, s, "--ro-bind-try")) continue;
        if (i + 2 >= argv.len) continue;
        if (!std.mem.eql(u8, argv[i + 1], argv[i + 2])) continue;
        var declared = false;
        for (sandbox_args.RO_SYSTEM_PATHS) |p| {
            if (std.mem.eql(u8, argv[i + 1], p)) declared = true;
        }
        try std.testing.expect(declared);
        seen += 1;
    }
    try std.testing.expectEqual(sandbox_args.RO_SYSTEM_PATHS.len, seen);
}

test "test_architecture_doc_matches_the_contract" {
    // Dimension 3.4 — the architecture doc's path table is checked against the
    // constant, not trusted to track it by review.
    const alloc = std.testing.allocator;
    const doc = std.Io.Dir.cwd().readFileAlloc(common.globalIo(), DOC_PATH, alloc, .limited(MAX_DOC_BYTES)) catch |err| switch (err) {
        // The runner test binary may run from a different cwd than the repo
        // root; a missing doc is an environment fact, not a contract failure.
        error.FileNotFound => return error.SkipZigTest,
        else => return err,
    };
    defer alloc.free(doc);

    for (sandbox_args.RO_SYSTEM_PATHS) |p| {
        const row = try std.fmt.allocPrint(alloc, "| `{s}` | read-only |", .{p});
        defer alloc.free(row);
        std.testing.expect(std.mem.indexOf(u8, doc, row) != null) catch |err| {
            std.debug.print("contract path missing from {s}: {s}\n", .{ DOC_PATH, p });
            return err;
        };
    }
}

const DOC_PATH = "docs/architecture/runner_fleet.md";
const MAX_DOC_BYTES = 1024 * 1024;

test "the child home nests under the writable tmpfs floor" {
    // The comptime guard in protocol_bind.zig proves this at build time; this
    // pins the PROPERTY at runtime so the reason survives a refactor of the
    // guard. A home outside the floor is a home bwrap never builds and landlock
    // never grants — which is the fault the constant exists to close.
    var inside = false;
    for (contract.protocol.BASELINE_RW_TMPFS) |rw| {
        if (std.mem.startsWith(u8, contract.protocol.CHILD_HOME, rw) and
            contract.protocol.CHILD_HOME.len > rw.len and
            contract.protocol.CHILD_HOME[rw.len] == '/') inside = true;
    }
    try std.testing.expect(inside);
}

test "the bwrap argv creates the child home on the floor it mounts" {
    // Order is load-bearing: --dir must follow the --tmpfs that owns the mount,
    // or the directory lands under the tmpfs and vanishes when it is mounted.
    const alloc = std.testing.allocator;
    const argv = try prefixWith(alloc, &.{});
    defer sandbox_args.freeArgv(alloc, argv);

    var tmpfs_at: ?usize = null;
    var dir_at: ?usize = null;
    for (argv, 0..) |a, i| {
        if (i + 1 >= argv.len) continue;
        if (std.mem.eql(u8, a, "--tmpfs") and std.mem.eql(u8, argv[i + 1], "/tmp")) tmpfs_at = i;
        if (std.mem.eql(u8, a, "--dir") and
            std.mem.eql(u8, argv[i + 1], contract.protocol.CHILD_HOME)) dir_at = i;
    }
    try std.testing.expect(tmpfs_at != null);
    try std.testing.expect(dir_at != null);
    try std.testing.expect(tmpfs_at.? < dir_at.?);
}
