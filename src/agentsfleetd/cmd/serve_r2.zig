//! Boot-time Cloudflare R2 client resolution. Reads the four R2 credentials from
//! the environment via env_resolve and builds the env-agnostic R2 client only when
//! all are present (else disabled). Split out of serve.zig to bound that file.

const std = @import("std");
const common = @import("common");
const R2 = @import("s3");
const env_resolve = @import("../config/env_resolve.zig");

const EnvMap = common.env.Map;

/// Resolve the R2 client from the environment, or null when disabled — any of the
/// four credentials unset/empty, or client init fails (creds present but broken).
/// Caller owns the result and must call `deinit`. Propagates OutOfMemory so boot
/// fails closed rather than silently disabling storage.
pub fn resolve(env_map: *const EnvMap, alloc: std.mem.Allocator, io: std.Io) std.mem.Allocator.Error!?R2 {
    const account = env_resolve.config(env_map, alloc, env_resolve.R2_ACCOUNT_ID_ENV);
    defer if (account) |v| alloc.free(v);
    const access_key = env_resolve.config(env_map, alloc, env_resolve.R2_ACCESS_KEY_ID_ENV);
    defer if (access_key) |v| alloc.free(v);
    const secret_key = env_resolve.config(env_map, alloc, env_resolve.R2_SECRET_ACCESS_KEY_ENV);
    defer if (secret_key) |v| alloc.free(v);
    const bucket = env_resolve.config(env_map, alloc, env_resolve.R2_BUCKET_ENV);
    defer if (bucket) |v| alloc.free(v);

    if (!present(account) or !present(access_key) or !present(secret_key) or !present(bucket)) return null;

    return R2.init(alloc, io, .{
        .account_id = account.?,
        .access_key_id = access_key.?,
        .secret_access_key = secret_key.?,
        .bucket = bucket.?,
    }) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => null,
    };
}

fn present(v: ?[]const u8) bool {
    return if (v) |s| s.len > 0 else false;
}
