//! Cloudflare R2 object store — a thin wrapper over the z3 S3 client (upstream
//! codeberg.org/fellowtraveler/z3, mirrored at agentsfleet/z3 and pinned in
//! build.zig.zon) for the two operations Fleet Bundle snapshots need: `put`
//! (import) and `get` (runner lease). Lives in src/lib so the agentsfleetd
//! (put) and runner (get) build graphs share one type identity (eng-review;
//! src/lib gating approved).
//!
//! Credentials (vault-fed at deploy) are read from the environment by the CALLER
//! and passed to `init` as a `Config`: this module imports only z3 + std and
//! cannot reach the daemon's `std.process.Init`-based env reader (Zig 0.16 removed
//! the direct env-read APIs). The boot path resolves R2_ACCOUNT_ID,
//! R2_ACCESS_KEY_ID, R2_SECRET_ACCESS_KEY, R2_BUCKET and constructs the client only
//! when all are present. Secret values are never logged (VLT).

const R2 = @This();

// Stored-allocator pattern: R2 owns the heap config strings (the built endpoint +
// duped credentials) that back the z3 client's borrowed []const u8 config fields,
// plus the z3 client itself. `deinit` frees every owned field.
alloc: std.mem.Allocator,
client: z3.S3Client,
bucket: []const u8,
account_id: []const u8,
access_key_id: []const u8,
secret_access_key: []const u8,
endpoint: []const u8,

pub const Error = error{
    R2InitFailed,
    R2PutFailed,
    R2GetFailed,
    R2NotFound,
};

/// Resolved R2 credentials. The caller reads these from the environment (via the
/// daemon's env reader) and passes them here; this module never touches env.
pub const Config = struct {
    account_id: []const u8,
    access_key_id: []const u8,
    secret_access_key: []const u8,
    bucket: []const u8,
};

// R2 fixes the AWS SigV4 region label to "auto". Account endpoints address the
// bucket in the path (path-style), so virtual_host_style stays false.
const REGION = "auto";
const HTTP_2XX: u16 = 100; // status / HTTP_2XX == 2 → 2xx family
const R2_IDLE_CONNECTION_LIMIT: usize = 0;

/// Build an R2 client from resolved credentials. `io` is the caller's io interface
/// (e.g. `constants.globalIo()`). Dupes every config string into owned storage —
/// the caller retains ownership of `cfg`. Caller owns the result and must `deinit`.
pub fn init(alloc: std.mem.Allocator, io: std.Io, cfg: Config) (Error || std.mem.Allocator.Error)!R2 {
    const account_id = try alloc.dupe(u8, cfg.account_id);
    errdefer alloc.free(account_id);
    const access_key_id = try alloc.dupe(u8, cfg.access_key_id);
    errdefer alloc.free(access_key_id);
    const secret_access_key = try alloc.dupe(u8, cfg.secret_access_key);
    errdefer alloc.free(secret_access_key);
    const bucket = try alloc.dupe(u8, cfg.bucket);
    errdefer alloc.free(bucket);

    const endpoint = try std.fmt.allocPrint(alloc, "https://{s}.r2.cloudflarestorage.com", .{account_id});
    errdefer alloc.free(endpoint);

    var client = z3.S3Client.init(alloc, .{
        .access_key_id = access_key_id,
        .secret_access_key = secret_access_key,
        .region = REGION,
        .endpoint = endpoint,
        .virtual_host_style = false,
    }, .{ .io = io }) catch return Error.R2InitFailed;
    client.http_client.connection_pool.free_size = R2_IDLE_CONNECTION_LIMIT;

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
    self.* = undefined;
}

/// Put an object (the immutable bundle snapshot). Keys are content-hash addressed,
/// so re-putting identical bytes is idempotent.
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

test "init survives an allocation failure at every step without leaking" {
    // Walk the multi-step init's whole errdefer ladder: fail the Nth allocation
    // for every N until init succeeds. Each iteration that fails mid-ladder
    // must free exactly what it had acquired — the FailingAllocator wraps the
    // testing allocator, which reports any leak as a test failure.
    var fail_index: usize = 0;
    while (fail_index < 64) : (fail_index += 1) {
        var fa = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = fail_index });
        var r2 = R2.init(fa.allocator(), std.testing.io, .{
            .account_id = "acct-ladder",
            .access_key_id = "ak-ladder",
            .secret_access_key = "sk-ladder",
            .bucket = "bkt-ladder",
        }) catch |err| {
            // Only the two named outcomes are acceptable: the allocator's own
            // OOM surfacing verbatim, or z3's init failure mapped to the error
            // this module's callers switch on.
            try std.testing.expect(err == error.OutOfMemory or err == Error.R2InitFailed);
            continue;
        };
        r2.deinit();
        return; // the ladder is exhausted — every prior index failed cleanly
    }
    return error.TestUnexpectedResult; // init never succeeded: not a ladder proof
}

test "init builds the account-scoped endpoint and dupes every credential" {
    var r2 = try R2.init(std.testing.allocator, std.testing.io, .{
        .account_id = "megam-acct",
        .access_key_id = "ak",
        .secret_access_key = "sk",
        .bucket = "snapshots",
    });
    defer r2.deinit();

    try std.testing.expectEqualStrings("https://megam-acct.r2.cloudflarestorage.com", r2.endpoint);
    try std.testing.expectEqualStrings("snapshots", r2.bucket);
    // Owned copies, not borrows of the caller's cfg — the caller may free its
    // strings the moment init returns.
    try std.testing.expectEqualStrings("megam-acct", r2.account_id);
    try std.testing.expectEqualStrings("ak", r2.access_key_id);
    try std.testing.expectEqualStrings("sk", r2.secret_access_key);
}

test "R2 disables idle HTTP connection reuse" {
    var r2 = try R2.init(std.testing.allocator, std.testing.io, .{
        .account_id = "",
        .access_key_id = "",
        .secret_access_key = "",
        .bucket = "",
    });
    defer r2.deinit();

    try std.testing.expectEqual(
        R2_IDLE_CONNECTION_LIMIT,
        r2.client.http_client.connection_pool.free_size,
    );
}

const std = @import("std");
const z3 = @import("z3");
