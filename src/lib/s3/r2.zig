//! Cloudflare R2 object store — a thin wrapper over the z3 S3 client
//! (codeberg.org/fellowtraveler/z3, pinned in build.zig.zon) for the two
//! operations Fleet Bundle snapshots need: `put` (import) and `get` (runner
//! lease). Lives in src/lib so the agentsfleetd (put) and runner (get) build
//! graphs share one type identity (eng-review; src/lib gating approved).
//!
//! Credentials come from the environment (vault-fed at deploy): R2_ACCOUNT_ID,
//! R2_ACCESS_KEY_ID, R2_SECRET_ACCESS_KEY, R2_BUCKET. `fromEnv` fails loud
//! (MissingR2Config) if any is unset/empty — the preflight credential gate and
//! this boot path both enforce presence. Secret values are never logged (VLT).

const R2 = @This();

// Stored-allocator pattern: R2 owns the heap config strings (env-read + the
// built endpoint + bucket) that back the z3 client's borrowed []const u8 config
// fields, plus the z3 client itself. `deinit` frees every owned field.
alloc: std.mem.Allocator,
client: z3.S3Client,
bucket: []const u8,
account_id: []const u8,
access_key_id: []const u8,
secret_access_key: []const u8,
endpoint: []const u8,

pub const Error = error{
    MissingR2Config,
    R2InitFailed,
    R2PutFailed,
    R2GetFailed,
    R2NotFound,
};

// R2 fixes the AWS SigV4 region label to "auto". Account endpoints address the
// bucket in the path (path-style), so virtual_host_style stays false.
const REGION = "auto";
const ENV_ACCOUNT = "R2_ACCOUNT_ID";
const ENV_ACCESS_KEY = "R2_ACCESS_KEY_ID";
const ENV_SECRET_KEY = "R2_SECRET_ACCESS_KEY";
const ENV_BUCKET = "R2_BUCKET";
const HTTP_2XX: u16 = 100; // status / HTTP_2XX == 2 → 2xx family

/// Build an R2 client from the environment. `io` is the caller's io interface
/// (e.g. `constants.globalIo()`). Returns MissingR2Config if any required env
/// var is unset/empty. Caller owns the result and must call `deinit`.
pub fn fromEnv(alloc: std.mem.Allocator, io: std.Io) (Error || std.mem.Allocator.Error)!R2 {
    const account_id = readEnv(alloc, ENV_ACCOUNT) orelse return Error.MissingR2Config;
    errdefer alloc.free(account_id);
    const access_key_id = readEnv(alloc, ENV_ACCESS_KEY) orelse return Error.MissingR2Config;
    errdefer alloc.free(access_key_id);
    const secret_access_key = readEnv(alloc, ENV_SECRET_KEY) orelse return Error.MissingR2Config;
    errdefer alloc.free(secret_access_key);
    const bucket = readEnv(alloc, ENV_BUCKET) orelse return Error.MissingR2Config;
    errdefer alloc.free(bucket);

    const endpoint = try std.fmt.allocPrint(alloc, "https://{s}.r2.cloudflarestorage.com", .{account_id});
    errdefer alloc.free(endpoint);

    const client = z3.S3Client.init(alloc, .{
        .access_key_id = access_key_id,
        .secret_access_key = secret_access_key,
        .region = REGION,
        .endpoint = endpoint,
        .virtual_host_style = false,
    }, .{ .io = io }) catch return Error.R2InitFailed;

    return .{
        .alloc = alloc,
        .client = client,
        .bucket = bucket,
        .account_id = account_id,
        .access_key_id = access_key_id,
        .secret_access_key = secret_access_key,
        .endpoint = endpoint,
    };
}

pub fn deinit(self: *R2) void {
    self.client.deinit();
    self.alloc.free(self.account_id);
    self.alloc.free(self.access_key_id);
    self.alloc.free(self.secret_access_key);
    self.alloc.free(self.endpoint);
    self.alloc.free(self.bucket);
}

/// Put an object (the immutable bundle snapshot). Keys are content-hash
/// addressed, so re-putting identical bytes is idempotent.
pub fn put(self: *R2, key: []const u8, body: []const u8) Error!void {
    var resp = self.client.putObject(self.bucket, key, body, .{}) catch return Error.R2PutFailed;
    defer resp.deinit();
    if (@intFromEnum(resp.http_head.status) / HTTP_2XX != 2) return Error.R2PutFailed;
}

/// Get an object's bytes. Caller owns the returned slice (allocated with `alloc`).
pub fn get(self: *R2, alloc: std.mem.Allocator, key: []const u8) Error![]u8 {
    var resp = self.client.getObject(self.bucket, key, .{}) catch return Error.R2GetFailed;
    defer resp.deinit();
    const status = @intFromEnum(resp.http_head.status);
    if (status == 404) return Error.R2NotFound;
    if (status / HTTP_2XX != 2) return Error.R2GetFailed;
    return alloc.dupe(u8, resp.body) catch return Error.R2GetFailed;
}

// Reads an env var into owned memory; null when unset OR empty (empty is treated
// as missing so a blank deploy var fails the same as an absent one).
fn readEnv(alloc: std.mem.Allocator, name: []const u8) ?[]const u8 {
    const v = std.process.getEnvVarOwned(alloc, name) catch return null;
    if (v.len == 0) {
        alloc.free(v);
        return null;
    }
    return v;
}

const std = @import("std");
const z3 = @import("z3");
