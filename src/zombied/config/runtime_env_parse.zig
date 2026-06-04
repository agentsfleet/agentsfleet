// Generic env-var readers for runtime config loading.
//
// Each helper takes the target ValidationError variant as a parameter, so
// this module stays decoupled from the specific error names — callers
// decide which variant maps to a missing/malformed env var.
//
// Zig 0.16 removed `std.process.getEnvVarOwned`; the environment is threaded
// as an `Environ.Map` snapshot and read through the shared `common.env.owned`
// facade. A truly-absent var → null → default/missing; OOM propagates (it is
// no longer masked as "unset").

const std = @import("std");
const common = @import("common");
const runtime_types = @import("runtime_types.zig");

const ValidationError = runtime_types.ValidationError;

/// Return the env var as an owned heap slice, or `missing_error` if unset.
/// Caller must free the returned slice.
pub fn requiredEnvOwned(
    env_map: *const std.process.Environ.Map,
    alloc: std.mem.Allocator,
    name: []const u8,
    missing_error: ValidationError,
) ![]const u8 {
    return (try common.env.owned(env_map, alloc, name)) orelse return missing_error;
}

/// Return the env var as an owned heap slice, or an owned dupe of
/// `default_value` if the env var is unset. Caller must free.
pub fn envOrDefaultOwned(
    env_map: *const std.process.Environ.Map,
    alloc: std.mem.Allocator,
    name: []const u8,
    default_value: []const u8,
) ![]const u8 {
    return (try common.env.owned(env_map, alloc, name)) orelse try alloc.dupe(u8, default_value);
}

pub fn parseU16Env(
    env_map: *const std.process.Environ.Map,
    alloc: std.mem.Allocator,
    name: []const u8,
    default_value: u16,
    invalid_error: ValidationError,
) !u16 {
    const raw = (try common.env.owned(env_map, alloc, name)) orelse return default_value;
    defer alloc.free(raw);
    return std.fmt.parseInt(u16, raw, 10) catch invalid_error;
}

pub fn parseU32Env(
    env_map: *const std.process.Environ.Map,
    alloc: std.mem.Allocator,
    name: []const u8,
    default_value: u32,
    invalid_error: ValidationError,
) !u32 {
    const raw = (try common.env.owned(env_map, alloc, name)) orelse return default_value;
    defer alloc.free(raw);
    return std.fmt.parseInt(u32, raw, 10) catch invalid_error;
}

pub fn parseI16Env(
    env_map: *const std.process.Environ.Map,
    alloc: std.mem.Allocator,
    name: []const u8,
    default_value: i16,
    invalid_error: ValidationError,
) !i16 {
    const raw = (try common.env.owned(env_map, alloc, name)) orelse return default_value;
    defer alloc.free(raw);
    return std.fmt.parseInt(i16, raw, 10) catch invalid_error;
}

pub fn parseOptionalI64Env(
    env_map: *const std.process.Environ.Map,
    alloc: std.mem.Allocator,
    name: []const u8,
    invalid_error: ValidationError,
) !?i64 {
    const raw = (try common.env.owned(env_map, alloc, name)) orelse return null;
    defer alloc.free(raw);
    return std.fmt.parseInt(i64, raw, 10) catch invalid_error;
}
