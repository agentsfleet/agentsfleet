//! Minimal argv reader for the operator subcommands. Space-separated flags
//! (`--api <url>`), matching `zombiectl`'s convention — distinct from
//! `child_exec`'s `--workspace=` `=`-form, which is the forked-child protocol,
//! not an operator surface. argv is never secret (the admin JWT and `zrn_` come
//! from the environment, not flags, by default — RULE VLT).

const std = @import("std");

/// Value of the argv entry following `--name`, or null if absent / no value.
pub fn opt(name: []const u8) ?[]const u8 {
    const argv = std.os.argv;
    var i: usize = 1;
    while (i + 1 < argv.len) : (i += 1) {
        if (std.mem.eql(u8, std.mem.span(argv[i]), name)) return std.mem.span(argv[i + 1]);
    }
    return null;
}

/// True when the bare flag `--name` appears anywhere in argv.
pub fn has(name: []const u8) bool {
    for (std.os.argv[1..]) |arg| {
        if (std.mem.eql(u8, std.mem.span(arg), name)) return true;
    }
    return false;
}

/// Resolve a value from `--flag` (preferred) else env var `env`, returning owned
/// memory (the flag value is duped) so callers `defer free` uniformly; null when
/// neither is set.
pub fn flagOrEnv(alloc: std.mem.Allocator, flag: []const u8, env: []const u8) ?[]const u8 {
    if (opt(flag)) |v| return alloc.dupe(u8, v) catch null;
    return std.process.getEnvVarOwned(alloc, env) catch null;
}

/// Owned env-var value, or null if unset.
pub fn envOwned(alloc: std.mem.Allocator, env: []const u8) ?[]const u8 {
    return std.process.getEnvVarOwned(alloc, env) catch null;
}
