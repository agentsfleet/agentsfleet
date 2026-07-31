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
const sandbox = @import("sandbox_args.zig");

const HOME = "HOME";
const HOME_VALUE = "/run/agentsfleet";
const DENIED_VAR = "AGENTSFLEET_RUNNER_TOKEN";
const DENIED_VALUE = "agt_r_not_a_real_token";
const OFF_ALLOWLIST_VAR = "SHELL";

test "test_home_reaches_sandboxed_child: HOME crosses when the daemon has it, credentials never do" {
    const alloc = std.testing.allocator;

    var daemon_env: std.process.Environ.Map = .init(alloc);
    defer daemon_env.deinit();
    try daemon_env.put(HOME, HOME_VALUE);
    try daemon_env.put(DENIED_VAR, DENIED_VALUE);
    try daemon_env.put(OFF_ALLOWLIST_VAR, "/bin/zsh");

    var child_env = try child_process.buildChildEnviron(alloc, &daemon_env);
    defer child_env.deinit();

    const home = child_env.get(HOME) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings(HOME_VALUE, home);
    // Fail-closed: the control-plane credential and anything else off the
    // allowlist are absent, not merely empty.
    try std.testing.expect(child_env.get(DENIED_VAR) == null);
    try std.testing.expect(child_env.get(OFF_ALLOWLIST_VAR) == null);
}

test "an unset HOME is never substituted or defaulted" {
    const alloc = std.testing.allocator;

    var daemon_env: std.process.Environ.Map = .init(alloc);
    defer daemon_env.deinit();
    try daemon_env.put(DENIED_VAR, DENIED_VALUE);

    var child_env = try child_process.buildChildEnviron(alloc, &daemon_env);
    defer child_env.deinit();

    // The regression this pins: with no HOME on the daemon the child gets none,
    // its config load fails, and no lease can run. The fix lives in the unit
    // file, not here — this test exists so that stays a deliberate contract
    // rather than something a future allowlist edit appears to paper over.
    try std.testing.expect(child_env.get(HOME) == null);
}

test "HOME is on the passthrough allowlist and the deny prefix is not" {
    var saw_home = false;
    for (sandbox.ENV_PASSTHROUGH_ALLOWLIST) |name| {
        if (std.mem.eql(u8, name, HOME)) saw_home = true;
        try std.testing.expect(!std.mem.startsWith(u8, name, sandbox.ENV_DENY_PREFIX));
    }
    try std.testing.expect(saw_home);
}
