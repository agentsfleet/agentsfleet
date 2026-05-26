// Generic env-var readers for runtime config loading.
//
// Each helper takes the target ValidationError variant as a parameter, so
// this module stays decoupled from the specific error names — callers
// decide which variant maps to a missing/malformed env var.

const std = @import("std");
const runtime_types = @import("runtime_types.zig");

const ValidationError = runtime_types.ValidationError;

/// Return the env var as an owned heap slice, or `missing_error` if unset.
/// Caller must free the returned slice.
pub fn requiredEnvOwned(
    alloc: std.mem.Allocator,
    name: []const u8,
    missing_error: ValidationError,
) ![]u8 {
    return std.process.getEnvVarOwned(alloc, name) catch missing_error;
}

/// Return the env var as an owned heap slice, or an owned dupe of
/// `default_value` if the env var is unset. Caller must free.
pub fn envOrDefaultOwned(
    alloc: std.mem.Allocator,
    name: []const u8,
    default_value: []const u8,
) ![]u8 {
    return std.process.getEnvVarOwned(alloc, name) catch try alloc.dupe(u8, default_value);
}

pub fn parseU16Env(
    alloc: std.mem.Allocator,
    name: []const u8,
    default_value: u16,
    invalid_error: ValidationError,
) !u16 {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return default_value;
    defer alloc.free(raw);
    return std.fmt.parseInt(u16, raw, 10) catch invalid_error;
}

pub fn parseU32Env(
    alloc: std.mem.Allocator,
    name: []const u8,
    default_value: u32,
    invalid_error: ValidationError,
) !u32 {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return default_value;
    defer alloc.free(raw);
    return std.fmt.parseInt(u32, raw, 10) catch invalid_error;
}

pub fn parseI16Env(
    alloc: std.mem.Allocator,
    name: []const u8,
    default_value: i16,
    invalid_error: ValidationError,
) !i16 {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return default_value;
    defer alloc.free(raw);
    return std.fmt.parseInt(i16, raw, 10) catch invalid_error;
}

pub fn parseOptionalI64Env(
    alloc: std.mem.Allocator,
    name: []const u8,
    invalid_error: ValidationError,
) !?i64 {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return null;
    defer alloc.free(raw);
    return std.fmt.parseInt(i64, raw, 10) catch invalid_error;
}
