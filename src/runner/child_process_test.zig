//! Coverage for the sandboxed child's environment construction.
//!
//! Two properties matter here and they pull in opposite directions. The child
//! must receive `HOME`, because the folded-in NullClaw engine resolves its
//! config directory from it and fails closed without one — that failure kills
//! every lease at init, before the fleet runs. The child must NOT receive the
//! daemon's control-plane credentials. `buildChildEnviron` is the single place
//! both are decided, so both are proven here.
//!
//! Note what the second test pins: nothing invents a `HOME`. When the daemon
//! has none, the child gets none. That is why the systemd unit
//! (`deploy/baremetal/agentsfleet-runner.service`) has to supply it — systemd
//! gives a `User=`-less service no `HOME` of its own.

const std = @import("std");
const child_process = @import("child_process.zig");
const sandbox_env = @import("sandbox_env.zig");
const contract = @import("contract");

const HOME = "HOME";
const HOME_VALUE = "/run/agentsfleet";
const DENIED_VAR = "AGENTSFLEET_RUNNER_TOKEN";
const DENIED_VALUE = "agt_r_not_a_real_token";
const OFF_ALLOWLIST_VAR = "SHELL";

test "test_home_reaches_sandboxed_child: the sandbox's HOME crosses, the daemon's never does" {
    const alloc = std.testing.allocator;

    var daemon_env: std.process.Environ.Map = .init(alloc);
    defer daemon_env.deinit();
    try daemon_env.put(HOME, HOME_VALUE);
    try daemon_env.put(DENIED_VAR, DENIED_VALUE);
    try daemon_env.put(OFF_ALLOWLIST_VAR, "/bin/zsh");

    var child_env = try child_process.buildChildEnviron(alloc, &daemon_env);
    defer child_env.deinit();

    const home = child_env.get(HOME) orelse return error.TestUnexpectedResult;
    // HOME_VALUE is the unit's real setting, and forwarding it is the defect:
    // it names a host RuntimeDirectory outside every bind and landlock rule, so
    // a child holding it dies at AccessDenied. The child gets the sandbox's own.
    try std.testing.expectEqualStrings(contract.protocol.CHILD_HOME, home);
    try std.testing.expect(!std.mem.eql(u8, HOME_VALUE, home));
    // Fail-closed: the control-plane credential and anything else off the
    // allowlist are absent, not merely empty.
    try std.testing.expect(child_env.get(DENIED_VAR) == null);
    try std.testing.expect(child_env.get(OFF_ALLOWLIST_VAR) == null);
}

test "an unset HOME is still assigned, never left for the child to resolve" {
    const alloc = std.testing.allocator;

    var daemon_env: std.process.Environ.Map = .init(alloc);
    defer daemon_env.deinit();
    try daemon_env.put(DENIED_VAR, DENIED_VALUE);

    var child_env = try child_process.buildChildEnviron(alloc, &daemon_env);
    defer child_env.deinit();

    // This test previously pinned the OPPOSITE: with no daemon HOME the child got
    // none. That made a failure mode into a contract — a child with no HOME cannot
    // resolve a config directory, so no lease could run. The sandbox now assigns
    // one, so the daemon's environment stops deciding whether a lease is possible.
    try std.testing.expectEqualStrings(contract.protocol.CHILD_HOME, child_env.get(HOME).?);
}

test "HOME is off the passthrough allowlist, and the deny prefix stays off it" {
    // The inverse of what this once asserted. Forwarding the daemon's HOME is
    // what pointed the child at /run/agentsfleet — a host path outside every bind
    // and landlock rule — so the allowlist must NOT carry it: an entry here would
    // land the daemon's value in the map and re-break every lease.
    for (sandbox_env.ENV_PASSTHROUGH_ALLOWLIST) |name| {
        try std.testing.expect(!std.mem.eql(u8, name, HOME));
        try std.testing.expect(!std.mem.startsWith(u8, name, sandbox_env.ENV_DENY_PREFIX));
    }
}
