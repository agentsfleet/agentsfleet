//! Redis client. File-as-struct: the file IS the `Client` type.
//!
//! Thin façade over `Pool`. `Client.command` acquires a connection from
//! the pool, runs one RESP round-trip, releases. Transport errors are
//! non-resumable: the pool closes the connection and `command` retries
//! once with a fresh dial (MAX_ATTEMPTS = 2). Server-side `.err` replies
//! are resumable: the connection stays alive in the pool and the caller
//! surfaces the error.
//!
//! No mutex — concurrent callers each take their own pooled connection
//! and never serialize across the network round-trip (Invariant 1).
//! Self-heal on idle-drop is a Pool property: a dead idle conn poisons
//! on first IO error, gets closed by release, and the retry layer dials
//! fresh.

pub const Client = @This();

const S_PONG = "PONG";
const S_SET = "SET";
const S_EX = "EX";
const S_D = "{d}";
const S_PING = "PING";
const S_OK = "OK";
const S_XADD_ZOMBIE_EVENT_FAILED = "xadd_zombie_event_failed";

// XADD argv slots for `xaddZombieEvent` — lifted to file scope so the
// compile-folded prefix is a single comptime slice instead of six slot
// assignments at runtime. The `MAXLEN ~ 10000` triplet caps the
// zombie:{id}:events stream's retention (~10k approximate trim); `*`
// asks Redis to generate the stream entry id (which IS the event_id).
const XADD_VERB: []const u8 = "XADD";
const XADD_MAXLEN_KEYWORD: []const u8 = "MAXLEN";
const XADD_MAXLEN_APPROX: []const u8 = "~";
const XADD_MAXLEN_ZOMBIE_EVENTS: []const u8 = "10000";
const XADD_AUTO_ID: []const u8 = "*";

/// Compile-folded tail for `XADD zombie:{id}:events MAXLEN ~ 10000 * …`.
/// Slot 0 = `XADD`, slot 1 = stream key (runtime), slots 2..6 = this slice.
const XADD_ZOMBIE_TRIM_TAIL: []const []const u8 = &.{
    XADD_MAXLEN_KEYWORD,
    XADD_MAXLEN_APPROX,
    XADD_MAXLEN_ZOMBIE_EVENTS,
    XADD_AUTO_ID,
};
const XADD_ZOMBIE_PREFIX_LEN: usize = 2 + XADD_ZOMBIE_TRIM_TAIL.len;

/// Per spec retry contract: pool-path operations get 2 attempts total
/// before the error surfaces to the caller. No backoff at this layer —
/// the caller (PG-dedup'd XADD, lossy PUBLISH, idempotent XACK) tolerates
/// at-least-once delivery.
const MAX_ATTEMPTS: u8 = 2;

pub const InitOptions = struct {
    /// `SO_RCVTIMEO` for every pooled Connection. Null = block forever
    /// (legacy / test harness). Production callers pass the env-derived
    /// value so a frozen Upstash proxy can't pin a worker thread.
    read_timeout_ms: ?u32 = null,
    /// Custom TLS CA bundle path → `Config.ca_cert_file` (the URL-only connect
    /// paths carry no env snapshot). Test harnesses pass the broker's self-signed
    /// cert; the env-map path resolves it from `REDIS_TLS_CA_CERT_FILE` instead.
    /// Null = system trust store.
    ca_cert_file: ?[]const u8 = null,
};

alloc: std.mem.Allocator,
pool: Pool,

pub fn connectFromEnv(io: std.Io, env_map: *const EnvMap, alloc: std.mem.Allocator, role: redis_types.RedisRole) !Client {
    return connectFromEnvWithOptions(io, env_map, alloc, role, .{});
}

pub fn connectFromEnvWithOptions(io: std.Io, env_map: *const EnvMap, alloc: std.mem.Allocator, role: redis_types.RedisRole, options: InitOptions) !Client {
    const url_owned = try redis_config.resolveRedisUrl(env_map, alloc, role);
    defer alloc.free(url_owned);
    var cfg = try redis_config.parseRedisUrl(alloc, url_owned);
    {
        // Resolve the optional custom CA path once from the env snapshot and
        // hand ownership to cfg. Scoped errdefer covers only this window —
        // once finishFromConfig calls Pool.init, the pool owns cfg.
        errdefer redis_config.deinitConfig(alloc, cfg);
        cfg.ca_cert_file = try common.env.owned(env_map, alloc, redis_config.CA_CERT_FILE_ENV);
    }
    return finishFromConfig(io, alloc, cfg, options);
}

pub fn connectFromUrl(io: std.Io, alloc: std.mem.Allocator, url: []const u8) !Client {
    return connectFromUrlWithOptions(io, alloc, url, .{});
}

pub fn connectFromUrlWithOptions(io: std.Io, alloc: std.mem.Allocator, url: []const u8, options: InitOptions) !Client {
    var cfg = try redis_config.parseRedisUrl(alloc, url);
    {
        // Own the optional CA path on cfg; scoped errdefer covers only the dup
        // window — finishFromConfig's Pool.init takes ownership of cfg after.
        errdefer redis_config.deinitConfig(alloc, cfg);
        if (options.ca_cert_file) |ca| cfg.ca_cert_file = try alloc.dupe(u8, ca);
    }
    return finishFromConfig(io, alloc, cfg, options);
}

/// Take ownership of `cfg`, stand up the Pool, and wrap it in a Client.
/// NOTE: do NOT register an `errdefer deinitConfig(alloc, cfg)` here.
/// `Pool.init` takes ownership of cfg unconditionally — it has its own
/// `errdefer redis_config.deinitConfig(alloc, pool.cfg)` that fires on init
/// failure, AND owns the cfg in `pool.deinit()` on success. A second errdefer
/// at this layer double-frees cfg.host on Pool.init failure.
fn finishFromConfig(io: std.Io, alloc: std.mem.Allocator, cfg: redis_config.Config, options: InitOptions) !Client {
    var pool = try Pool.init(io, alloc, cfg, .{ .read_timeout_ms = options.read_timeout_ms });
    errdefer pool.deinit();

    log.info("connected", .{ .host = cfg.host, .port = cfg.port, .tls = cfg.use_tls });
    return .{ .alloc = alloc, .pool = pool };
}

pub fn deinit(self: *Client) void {
    self.pool.deinit();
}

pub fn publish(self: *Client, channel: []const u8, data: []const u8) !void {
    var resp = try self.command(&.{ "PUBLISH", channel, data });
    defer resp.deinit(self.alloc);
    switch (resp) {
        .integer => {},
        else => return error.RedisPublishFailed,
    }
    log.debug("publish", .{ .channel = channel, .data_len = data.len });
}

/// SET key value EX ttl_seconds — used for cancellation signals.
pub fn setEx(self: *Client, key: []const u8, value: []const u8, ttl_seconds: u32) !void {
    var ttl_buf: [16]u8 = undefined;
    const ttl_str = try std.fmt.bufPrint(&ttl_buf, S_D, .{ttl_seconds});
    var resp = try self.command(&.{ S_SET, key, value, S_EX, ttl_str });
    defer resp.deinit(self.alloc);
    switch (resp) {
        .simple => |v| if (!std.mem.eql(u8, v, S_OK)) return error.RedisSetExFailed,
        else => return error.RedisSetExFailed,
    }
}

pub fn ping(self: *Client) !void {
    var resp = try self.command(&.{S_PING});
    defer resp.deinit(self.alloc);
    switch (resp) {
        .simple => |v| if (!std.mem.eql(u8, v, S_PONG)) return error.RedisPingFailed,
        else => return error.RedisPingFailed,
    }
}

/// Liveness probe for `/readyz`. Pool retry handles dead idle conns
/// transparently — no explicit reconnect plumbing here. PING is
/// idempotent so the standard 2-attempt retry is safe.
pub fn readyCheck(self: *Client) !void {
    try self.ping();
}

pub fn aclWhoAmI(self: *Client) ![]u8 {
    var resp = try self.command(&.{ "ACL", "WHOAMI" });
    defer resp.deinit(self.alloc);
    const who = redis_protocol.valueAsString(resp) orelse return error.RedisUnexpectedResponse;
    return try self.alloc.dupe(u8, who);
}

/// DEL one key — releases an idempotency slot whose protected op failed post-claim.
pub fn del(self: *Client, key: []const u8) !void {
    var resp = try self.command(&.{ "DEL", key });
    defer resp.deinit(self.alloc);
    switch (resp) {
        .integer => {},
        else => return error.RedisDelFailed,
    }
}

/// setNx sets key=value with ttl only if key does not exist.
/// Returns true if key was set (new), false if key already existed (duplicate).
pub fn setNx(self: *Client, key: []const u8, value: []const u8, ttl_seconds: u32) !bool {
    var ttl_buf: [12]u8 = undefined;
    const ttl_str = try std.fmt.bufPrint(&ttl_buf, S_D, .{ttl_seconds});
    var resp = try self.commandAllowError(&.{ S_SET, key, value, "NX", S_EX, ttl_str });
    defer resp.deinit(self.alloc);
    return switch (resp) {
        .simple => |s| std.mem.eql(u8, s, S_OK),
        else => false,
    };
}

/// XADD an EventEnvelope onto `zombie:{envelope.zombie_id}:events`. The Redis
/// stream entry id IS the canonical event_id; this function returns it
/// allocated via `self.alloc` so the caller (e.g. `POST /messages`) can
/// surface it in the response body for SSE correlation.
///
/// Stream is trimmed approximately to MAXLEN 10000 entries.
pub fn xaddZombieEvent(self: *Client, envelope: EventEnvelope) ![]u8 {
    var stream_key_buf: [128]u8 = undefined;
    const stream_key = try std.fmt.bufPrint(&stream_key_buf, "zombie:{s}:events", .{envelope.zombie_id});

    const payload_argv = try envelope.encodeForXAdd(self.alloc);
    defer EventEnvelope.freeXAddArgv(self.alloc, payload_argv);

    var argv = try self.alloc.alloc([]const u8, XADD_ZOMBIE_PREFIX_LEN + payload_argv.len);
    defer self.alloc.free(argv);
    argv[0] = XADD_VERB;
    argv[1] = stream_key;
    @memcpy(argv[2..XADD_ZOMBIE_PREFIX_LEN], XADD_ZOMBIE_TRIM_TAIL);
    @memcpy(argv[XADD_ZOMBIE_PREFIX_LEN..], payload_argv);

    var resp = try self.command(argv);
    defer resp.deinit(self.alloc);

    const id_str = switch (resp) {
        .bulk => |v| v orelse {
            log.err(S_XADD_ZOMBIE_EVENT_FAILED, .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .zombie_id = envelope.zombie_id, .actor = envelope.actor });
            return error.RedisXaddFailed;
        },
        else => {
            log.err(S_XADD_ZOMBIE_EVENT_FAILED, .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .zombie_id = envelope.zombie_id, .actor = envelope.actor });
            return error.RedisXaddFailed;
        },
    };
    const owned_id = try self.alloc.dupe(u8, id_str);
    log.debug("xadd_zombie_event", .{ .zombie_id = envelope.zombie_id, .event_id = owned_id, .actor = envelope.actor, .type = envelope.event_type.toSlice() });
    return owned_id;
}

/// Pool-backed: acquire → run command → release. Transport errors retry
/// up to MAX_ATTEMPTS with a fresh dial; server-side `.err` replies are
/// resumable (no retry) and surface to the caller.
pub fn command(self: *Client, argv: []const []const u8) !redis_protocol.RespValue {
    var attempt: u8 = 0;
    while (true) : (attempt += 1) {
        const conn = try self.pool.acquire();
        const resp = conn.command(argv) catch |err| {
            // OOM during RESP read poisons the connection (see
            // redis_connection.zig:commandAllowError — the length/count
            // header is consumed before the body allocation fires, leaving
            // partial bytes in the transport buffer). `release(conn, true)`
            // is safe because `Pool.release` checks `conn.state != .active`
            // and closes the poisoned connection via the early branch; the
            // conn is NOT returned to idle. OOM still surfaces verbatim to
            // the caller (no `ReadFailed` re-tag) so memory pressure shows
            // up with its real root cause.
            if (err == error.OutOfMemory) {
                self.pool.release(conn, true);
                return error.OutOfMemory;
            }
            // @errorCast narrows from Connection.Error (incl. OutOfMemory)
            // to RedisError — safe because the OOM branch returned above.
            const resumable = redis_errors.isResumable(@errorCast(err));
            self.pool.release(conn, resumable);
            if (resumable) {
                // Warn (not err): a resumable server-side reply (BUSYGROUP,
                // WRONGTYPE, READONLY) is degraded control flow — the inner
                // `redis_command_err_reply` log already captures the server
                // message at warn. Outer + inner stay at the same level so
                // negative-path tests don't trip Zig's "logged errors" gate.
                log.warn("command_error", .{ .cmd = if (argv.len > 0) argv[0] else "unknown", .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED });
                return err;
            }
            if (attempt + 1 >= MAX_ATTEMPTS) return err;
            self.pool.recordReconnect();
            continue;
        };
        self.pool.release(conn, true);
        return resp;
    }
}

/// Same as `command` but surfaces `.err` RespValue intact for callers
/// that need to inspect the server's error message (XGROUP CREATE
/// returning BUSYGROUP on a known-created group, etc.).
pub fn commandAllowError(self: *Client, argv: []const []const u8) !redis_protocol.RespValue {
    var attempt: u8 = 0;
    while (true) : (attempt += 1) {
        const conn = try self.pool.acquire();
        const resp = conn.commandAllowError(argv) catch |err| {
            // OOM passthrough — see command() above.
            if (err == error.OutOfMemory) {
                self.pool.release(conn, true);
                return error.OutOfMemory;
            }
            const resumable = redis_errors.isResumable(@errorCast(err));
            self.pool.release(conn, resumable);
            if (resumable or attempt + 1 >= MAX_ATTEMPTS) return err;
            self.pool.recordReconnect();
            continue;
        };
        self.pool.release(conn, true);
        return resp;
    }
}

/// Buffer size for `stableConsumerId`: prefix + '-' + max hostname.
pub const CONSUMER_ID_BUF_LEN: usize = queue_consts.consumer_prefix.len + 1 + std.posix.HOST_NAME_MAX;

/// Stable consumer identity for the zombie event streams: one per zombied
/// instance (host-derived, timestamp-free), so delivered-but-unacked entries
/// stay recoverable in this consumer's PEL and group cardinality stays
/// bounded — the retired per-probe minting orphaned every entry. The memcpys
/// are infallible by construction: CONSUMER_ID_BUF_LEN = prefix + max hostname.
pub fn stableConsumerId(buf: *[CONSUMER_ID_BUF_LEN]u8) []const u8 {
    const prefix = queue_consts.consumer_prefix ++ "-";
    var host_buf: [std.posix.HOST_NAME_MAX]u8 = undefined;
    // gethostname failure collapses this instance onto the shared `localhost`
    // consumer (one PEL for any such instance). Correctness is unaffected — the
    // per-zombie Postgres affinity claim is the only lease serializer — but
    // recovery attribution blurs, so the fallback is loud.
    const host = std.posix.gethostname(&host_buf) catch |err| blk: {
        log.warn("consumer_id_hostname_fallback", .{ .err = @errorName(err) });
        break :blk "localhost";
    };
    @memcpy(buf[0..prefix.len], prefix);
    @memcpy(buf[prefix.len..][0..host.len], host);
    return buf[0 .. prefix.len + host.len];
}

const std = @import("std");
const common = @import("common");
const EnvMap = common.env.Map;
const logging = @import("log");
const queue_consts = @import("constants.zig");
const redis_types = @import("redis_types.zig");
const redis_config = @import("redis_config.zig");
const redis_protocol = @import("redis_protocol.zig");
const redis_errors = @import("redis_errors.zig");
const Pool = @import("redis_pool.zig");
const error_codes = @import("../errors/error_registry.zig");
const EventEnvelope = @import("contract").event_envelope;
const log = logging.scoped(.redis_queue);
