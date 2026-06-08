const std = @import("std");
const builtin = @import("builtin");

pub fn module(
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
    dep_name: []const u8,
) *std.Build.Module {
    const target_os = target.result.os.tag;
    const target_arch = target.result.cpu.arch;
    const host_is_linux = builtin.os.tag == .linux;
    const host_is_darwin = builtin.os.tag == .macos;
    const same_arch = builtin.cpu.arch == target_arch;
    const openssl_override = b.option(bool, "openssl", "Force pg OpenSSL/TLS on/off (default: host/target auto-detect)");
    const enable_openssl = openssl_override orelse ((host_is_linux and target_os == .linux and same_arch) or (host_is_darwin and target_os == .macos));

    const pg_dep = if (enable_openssl) blk: {
        const homebrew_prefix = if (builtin.cpu.arch == .aarch64) "/opt/homebrew" else "/usr/local";
        const ssl_include: std.Build.LazyPath = .{ .cwd_relative = if (host_is_linux)
            "/usr/include"
        else
            homebrew_prefix ++ "/opt/openssl@3/include" };
        const ssl_lib: std.Build.LazyPath = .{ .cwd_relative = if (host_is_linux)
            b.fmt("/usr/lib/{s}", .{@tagName(builtin.cpu.arch) ++ "-linux-gnu"})
        else
            homebrew_prefix ++ "/opt/openssl@3/lib" };
        break :blk b.dependency(dep_name, .{
            .target = target,
            .optimize = optimize,
            .openssl_include_path = ssl_include,
            .openssl_lib_path = ssl_lib,
        });
    } else b.dependency(dep_name, .{
        .target = target,
        .optimize = optimize,
    });
    const pg_mod = pg_dep.module(dep_name);

    if (enable_openssl and host_is_linux) {
        pg_mod.addIncludePath(.{
            .cwd_relative = b.fmt("/usr/include/{s}-linux-gnu", .{@tagName(builtin.cpu.arch)}),
        });
    }

    return pg_mod;
}
