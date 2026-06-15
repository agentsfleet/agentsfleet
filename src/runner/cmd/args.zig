//! Minimal argv reader for the operator subcommands. Space-separated flags
//! (`--api <url>`), matching `agentsfleet`'s convention — distinct from
//! `child_exec`'s `--workspace=` `=`-form, which is the forked-child protocol,
//! not an operator surface. argv is never secret (the admin JWT and `agt_r` come
//! from the environment, not flags, by default — RULE VLT).

const std = @import("std");
const common = @import("common");

/// Value of the argv entry following `--name`, or null if absent / no value.
pub fn opt(argv: []const [:0]const u8, name: []const u8) ?[]const u8 {
    var i: usize = 1;
    while (i + 1 < argv.len) : (i += 1) {
        if (std.mem.eql(u8, argv[i], name)) return argv[i + 1];
    }
    return null;
}

/// True when the bare flag `--name` appears anywhere in argv.
pub fn has(argv: []const [:0]const u8, name: []const u8) bool {
    for (argv[1..]) |arg| {
        if (std.mem.eql(u8, arg, name)) return true;
    }
    return false;
}

/// Resolve a value from `--flag` (preferred) else env var `env`, returning owned
/// memory (the flag value is duped) so callers `defer free` uniformly. `null` =
/// neither is set; an error (`OutOfMemory`/`InvalidWtf8`) is propagated, NOT
/// masked as "unset" — so OOM surfaces as OOM, not a misleading "not set".
pub fn flagOrEnv(
    env_map: *const std.process.Environ.Map,
    argv: []const [:0]const u8,
    alloc: std.mem.Allocator,
    flag: []const u8,
    env: []const u8,
) !?[]const u8 {
    if (opt(argv, flag)) |v| return try alloc.dupe(u8, v);
    return envOwned(env_map, alloc, env);
}

/// Owned env-var value, or `null` if unset; OOM propagates. Zig 0.16 removed
/// `std.process.getEnvVarOwned`; the environment is threaded as an `Environ.Map`.
pub fn envOwned(env_map: *const std.process.Environ.Map, alloc: std.mem.Allocator, env: []const u8) !?[]const u8 {
    return common.env.owned(env_map, alloc, env);
}
