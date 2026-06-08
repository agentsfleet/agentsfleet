//! Redis-backed CRUD for CLI device-flow auth sessions. Sessions live at
//! `auth:session:{id}` with a 5-min TTL. The verification_pending → consumed
//! transition runs in one Lua EVAL (Invariant 15). Audit emit lives in the
//! handler layer.

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const queue_redis = @import("../queue/redis.zig");
const id_format = @import("../types/id_format.zig");
const hmac_sig = @import("hmac_sig");
const session_state = @import("../auth/session_state.zig");
const proto = @import("session_store_redis_proto.zig");
const REDIS_EVAL_COMMAND = "EVAL";
const HTTP_METHOD_GET = "GET";

const log = logging.scoped(.auth);

pub const SessionState = session_state.SessionState;
pub const SessionStatus = session_state.SessionStatus;
pub const VerifyOutcome = proto.VerifyOutcome;
pub const VerifyPayload = proto.VerifyPayload;
pub const DeleteOutcome = proto.DeleteOutcome;
pub const Error = proto.Error;

pub const SESSION_KEY_PREFIX: []const u8 = "auth:session:";
pub const SESSION_TTL_SECONDS: u32 = 300;
pub const CONSUME_REPLAY_WINDOW_MS: i64 = 60_000;
pub const MAX_VERIFY_ATTEMPTS: u8 = 5;
pub const TOKEN_NAME_MAX_LEN: usize = 64;
// cli_public_key (unauthenticated POST /sessions): same P-256 SPKI base64url
// shape as the dashboard pubkey; bound it so a caller can't park an oversized
// blob in Redis for the full TTL.
pub const CLI_PUBLIC_KEY_MAX_LEN: usize = 200;
// approve() envelope caps. Production ceilings: P-256 SPKI base64url is
// 124 chars; AES-256-GCM nonce base64url is 16 chars; ciphertext tracks
// Clerk JWT size at ~2 KB + 16 B tag. Caps bound an authenticated-but-
// malicious actor's ability to stash blobs in Redis per session.
pub const DASHBOARD_PUBKEY_MAX_LEN: usize = 200;
pub const NONCE_MAX_LEN: usize = 32;
pub const CIPHERTEXT_MAX_LEN: usize = 4096;

pub const SESSION_KEY_BUF_LEN: usize = SESSION_KEY_PREFIX.len + 36;
const SCAN_PAGE_BUF: usize = 128;

pub const SessionStore = struct {
    alloc: std.mem.Allocator,
    client: *queue_redis.Client,
    session_code_pepper: []const u8,
    audit_log_pepper: []const u8,

    pub fn init(
        alloc: std.mem.Allocator,
        client: *queue_redis.Client,
        session_code_pepper: []const u8,
        audit_log_pepper: []const u8,
    ) SessionStore {
        return .{
            .alloc = alloc,
            .client = client,
            .session_code_pepper = session_code_pepper,
            .audit_log_pepper = audit_log_pepper,
        };
    }

    /// Caller owns the returned session_id slice; pair with `alloc.free`.
    pub fn create(
        self: *SessionStore,
        cli_public_key: []const u8,
        token_name: []const u8,
    ) ![]const u8 {
        try validateCreateInputs(cli_public_key, token_name);

        const session_id = try id_format.allocUuidV7(self.alloc);
        errdefer self.alloc.free(session_id);

        const now_ms = clock.nowMillis();
        const state = SessionState{
            .session_id = session_id,
            .status = .pending,
            .cli_public_key = cli_public_key,
            .token_name = token_name,
            .created_at_ms = now_ms,
            .expires_at_ms = now_ms + session_state.SESSION_TTL_MS,
        };
        const blob = try session_state.encode(self.alloc, state);
        defer self.alloc.free(blob);

        var key_buf: [SESSION_KEY_BUF_LEN]u8 = undefined;
        const key = try formatSessionKey(&key_buf, session_id);
        try self.client.setEx(key, blob, SESSION_TTL_SECONDS);
        return session_id;
    }

    /// Caller owns the returned `Parsed`. Null = TTL evicted, never created,
    /// or explicit `DEL`. Handler maps to 410 / 404 as appropriate.
    pub fn get(self: *SessionStore, session_id: []const u8) !?std.json.Parsed(SessionState) {
        var key_buf: [SESSION_KEY_BUF_LEN]u8 = undefined;
        const key = try formatSessionKey(&key_buf, session_id);

        var resp = try self.client.command(&.{ HTTP_METHOD_GET, key });
        defer resp.deinit(self.alloc);
        const blob = switch (resp) {
            .bulk => |maybe| maybe orelse return null,
            else => return Error.UnexpectedRedisReply,
        };
        return try session_state.decode(self.alloc, blob);
    }

    /// Single-EVAL atomic approve. Any non-pending state at the moment of
    /// the read returns `Error.AlreadyApproved` / `SessionMissing`.
    pub fn approve(
        self: *SessionStore,
        session_id: []const u8,
        dashboard_public_key: []const u8,
        ciphertext: []const u8,
        nonce: []const u8,
        verification_code: []const u8,
        clerk_user_id: []const u8,
    ) Error!void {
        if (dashboard_public_key.len == 0 or dashboard_public_key.len > DASHBOARD_PUBKEY_MAX_LEN) {
            return Error.InvalidPublicKey;
        }
        if (ciphertext.len == 0 or ciphertext.len > CIPHERTEXT_MAX_LEN) {
            return Error.InvalidCipherText;
        }
        if (nonce.len == 0 or nonce.len > NONCE_MAX_LEN) return Error.InvalidNonce;
        if (verification_code.len != 6 or !isDigits(verification_code)) {
            return Error.InvalidVerificationCode;
        }

        var key_buf: [SESSION_KEY_BUF_LEN]u8 = undefined;
        const key = formatSessionKey(&key_buf, session_id) catch return Error.SessionMissing;

        var hmac_buf: [hmac_sig.MAC_LEN * 2]u8 = undefined;
        const hmac_hex = computeCodeHmacHex(&hmac_buf, self.session_code_pepper, session_id, verification_code);

        var ttl_buf: [12]u8 = undefined;
        var now_buf: [24]u8 = undefined;
        const ttl_str = std.fmt.bufPrint(&ttl_buf, "{d}", .{SESSION_TTL_SECONDS}) catch return Error.RedisError;
        const now_str = std.fmt.bufPrint(&now_buf, "{d}", .{clock.nowMillis()}) catch return Error.RedisError;

        var resp = self.client.command(&.{
            REDIS_EVAL_COMMAND,   proto.APPROVE_LUA, "1",     key,
            dashboard_public_key, ciphertext,        nonce,   hmac_hex,
            clerk_user_id,        now_str,           ttl_str,
        }) catch return Error.RedisError;
        defer resp.deinit(self.alloc);
        return proto.mapApproveOutcome(resp);
    }

    /// Single Lua EVAL: read blob, branch on state, HMAC-compare, atomic
    /// transition. Returns the union encoding what happened.
    pub fn verifyAndConsume(
        self: *SessionStore,
        session_id: []const u8,
        verification_code: []const u8,
        client_fingerprint_hex: []const u8,
    ) !VerifyOutcome {
        if (verification_code.len != 6 or !isDigits(verification_code)) {
            return Error.InvalidVerificationCode;
        }
        if (client_fingerprint_hex.len != session_state.HEX32_LEN) {
            return Error.UnexpectedRedisReply;
        }

        var key_buf: [SESSION_KEY_BUF_LEN]u8 = undefined;
        const key = try formatSessionKey(&key_buf, session_id);

        var hmac_buf: [hmac_sig.MAC_LEN * 2]u8 = undefined;
        const hmac_hex = computeCodeHmacHex(&hmac_buf, self.session_code_pepper, session_id, verification_code);

        var now_buf: [24]u8 = undefined;
        var win_buf: [12]u8 = undefined;
        var att_buf: [4]u8 = undefined;
        var ttl_buf: [12]u8 = undefined;
        const now_str = try std.fmt.bufPrint(&now_buf, "{d}", .{clock.nowMillis()});
        const win_str = try std.fmt.bufPrint(&win_buf, "{d}", .{CONSUME_REPLAY_WINDOW_MS});
        const att_str = try std.fmt.bufPrint(&att_buf, "{d}", .{MAX_VERIFY_ATTEMPTS});
        const ttl_str = try std.fmt.bufPrint(&ttl_buf, "{d}", .{SESSION_TTL_SECONDS});

        var resp = try self.client.command(&.{
            REDIS_EVAL_COMMAND, proto.VERIFY_AND_CONSUME_LUA, "1",                    key,
            hmac_hex,           now_str,                      client_fingerprint_hex, win_str,
            att_str,            ttl_str,
        });
        defer resp.deinit(self.alloc);
        // parseVerifyOutcome returns slices borrowed from `resp`; dupe so they
        // outlive the deinit. Caller pairs with `outcome.deinit(alloc)`.
        const borrowed = try proto.parseVerifyOutcome(resp);
        return try borrowed.dupe(self.alloc);
    }

    /// Atomic owner-checked abort with the caller-supplied `reason` (stored as
    /// the session's `aborted_reason`). Pass the same `audit_events.REASON_*`
    /// the handler emits so the stored reason cannot diverge from the audited
    /// one. Returns `.aborted` when this call performed the abort,
    /// `.already_aborted` when the session was already terminal (idempotent
    /// re-delete) so the caller can avoid emitting a duplicate audit record.
    pub fn delete(self: *SessionStore, session_id: []const u8, clerk_user_id: []const u8, reason: []const u8) Error!proto.DeleteOutcome {
        var key_buf: [SESSION_KEY_BUF_LEN]u8 = undefined;
        const key = formatSessionKey(&key_buf, session_id) catch return Error.SessionMissing;
        var ttl_buf: [12]u8 = undefined;
        const ttl_str = std.fmt.bufPrint(&ttl_buf, "{d}", .{SESSION_TTL_SECONDS}) catch return Error.RedisError;

        var resp = self.client.command(&.{
            REDIS_EVAL_COMMAND, proto.DELETE_OWNER_LUA, "1",     key,
            clerk_user_id,      reason,                 ttl_str,
        }) catch return Error.RedisError;
        defer resp.deinit(self.alloc);
        return proto.mapDeleteOutcome(resp);
    }

    /// Fires per aborted session inside `deleteAllForUser` so the handler
    /// can emit a per-session `.auth_audit` record. `session_id` is
    /// non-owned and only valid during the callback.
    pub const SessionAbortObserver = struct {
        ctx: *anyopaque,
        on_aborted: *const fn (*anyopaque, session_id: []const u8) void,
    };

    /// Abort every in-flight session owned by `clerk_user_id` with the
    /// caller-supplied `reason` (see `delete`). SCAN-based; O(total_sessions)
    /// but capped by the 5-min TTL.
    pub fn deleteAllForUser(
        self: *SessionStore,
        clerk_user_id: []const u8,
        reason: []const u8,
        observer: ?SessionAbortObserver,
    ) !u32 {
        return self.scanLoop(.{ .abort = .{ .clerk_user_id = clerk_user_id, .reason = reason, .observer = observer } });
    }

    /// Defensive scan — finds blobs whose `expires_at_ms` elapsed but whose
    /// Redis TTL was somehow cleared. Should always return 0 in steady state.
    pub fn runBackgroundSweep(self: *SessionStore, now_ms: i64) !u32 {
        const pruned = try self.scanLoop(.{ .prune = .{ .now_ms = now_ms } });
        if (pruned > 0) log.warn("session_sweep_pruned", .{ .pruned = pruned });
        return pruned;
    }

    const ScanOp = union(enum) {
        abort: struct {
            clerk_user_id: []const u8,
            reason: []const u8,
            observer: ?SessionAbortObserver,
        },
        prune: struct { now_ms: i64 },
    };

    fn scanLoop(self: *SessionStore, op: ScanOp) !u32 {
        var cursor: []const u8 = "0";
        var cursor_owned: ?[]u8 = null;
        defer if (cursor_owned) |c| self.alloc.free(c);
        var keys_buf: [SCAN_PAGE_BUF][]const u8 = undefined;
        var hits: u32 = 0;
        while (true) {
            var resp = try self.client.command(&.{ "SCAN", cursor, "MATCH", "auth:session:*", "COUNT", "100" });
            defer resp.deinit(self.alloc);
            const page = try proto.readScanReply(resp, &keys_buf);
            for (page.keys) |key| hits += try self.applyScanOp(op, key);
            if (cursor_owned) |c| self.alloc.free(c);
            cursor_owned = try self.alloc.dupe(u8, page.next_cursor);
            cursor = cursor_owned.?;
            if (std.mem.eql(u8, page.next_cursor, "0")) break;
        }
        return hits;
    }

    fn applyScanOp(self: *SessionStore, op: ScanOp, key: []const u8) !u32 {
        return switch (op) {
            .abort => |a| self.abortIfOwnedBy(key, a.clerk_user_id, a.reason, a.observer),
            .prune => |p| self.pruneIfExpired(key, p.now_ms),
        };
    }

    fn abortIfOwnedBy(
        self: *SessionStore,
        key: []const u8,
        clerk_user_id: []const u8,
        reason: []const u8,
        observer: ?SessionAbortObserver,
    ) !u32 {
        var ttl_buf: [12]u8 = undefined;
        const ttl_str = try std.fmt.bufPrint(&ttl_buf, "{d}", .{SESSION_TTL_SECONDS});
        var resp = try self.client.command(&.{
            REDIS_EVAL_COMMAND, proto.DELETE_OWNER_LUA, "1",     key,
            clerk_user_id,      reason,                 ttl_str,
        });
        defer resp.deinit(self.alloc);
        const tag = proto.firstTag(resp) orelse return 0;
        if (!std.mem.eql(u8, tag, "ok")) return 0;
        // `key` is `auth:session:{uuid}`; strip the prefix for the handler's audit emitter.
        if (observer) |o| if (key.len > SESSION_KEY_PREFIX.len) o.on_aborted(o.ctx, key[SESSION_KEY_PREFIX.len..]);
        return 1;
    }

    fn pruneIfExpired(self: *SessionStore, key: []const u8, now_ms: i64) !u32 {
        var resp = try self.client.command(&.{ HTTP_METHOD_GET, key });
        defer resp.deinit(self.alloc);
        const blob = switch (resp) {
            .bulk => |b| b orelse return 0,
            else => return 0,
        };
        var parsed = session_state.decode(self.alloc, blob) catch return 0;
        defer parsed.deinit();
        if (parsed.value.expires_at_ms > now_ms) return 0;
        var del = try self.client.command(&.{ "DEL", key });
        defer del.deinit(self.alloc);
        return 1;
    }
};

/// Validate create() inputs before any Redis I/O (pub for helpers_test boundary coverage).
pub fn validateCreateInputs(cli_public_key: []const u8, token_name: []const u8) Error!void {
    if (cli_public_key.len == 0 or cli_public_key.len > CLI_PUBLIC_KEY_MAX_LEN) return Error.InvalidPublicKey;
    if (token_name.len == 0 or token_name.len > TOKEN_NAME_MAX_LEN) return Error.InvalidTokenName;
}

pub fn formatSessionKey(buf: []u8, session_id: []const u8) ![]const u8 {
    if (!id_format.isUuidV7(session_id)) return error.InvalidSessionId;
    return std.fmt.bufPrint(buf, "{s}{s}", .{ SESSION_KEY_PREFIX, session_id });
}

pub fn isDigits(s: []const u8) bool {
    for (s) |c| if (c < '0' or c > '9') return false;
    return true;
}

pub fn computeCodeHmacHex(
    out: *[hmac_sig.MAC_LEN * 2]u8,
    pepper: []const u8,
    session_id: []const u8,
    code: []const u8,
) []const u8 {
    const mac = hmac_sig.computeMac(pepper, &.{ session_id, code });
    const hex = std.fmt.bytesToHex(mac, .lower);
    @memcpy(out, &hex);
    return out;
}

// Pure-function tests live in `session_store_redis_helpers_test.zig`.
test {
    _ = @import("session_store_redis_helpers_test.zig");
}
