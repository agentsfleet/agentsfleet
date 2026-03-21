//! In-memory CLI auth session store.
//! Sessions are ephemeral with a 5-minute TTL.
//! Used by the CLI login polling flow.

const std = @import("std");

const log = std.log.scoped(.auth);

pub const SessionStatus = enum {
    pending,
    complete,
    expired,
};

pub const Session = struct {
    session_id: [24]u8,
    status: SessionStatus,
    token: ?[]u8,
    created_at_ms: i64,
};

const ttl_ms: i64 = 5 * 60 * 1000;
const max_sessions: usize = 64;

pub const SessionStore = struct {
    alloc: std.mem.Allocator,
    mutex: std.Thread.Mutex = .{},
    sessions: std.StringHashMap(Session),

    pub fn init(alloc: std.mem.Allocator) SessionStore {
        return .{
            .alloc = alloc,
            .sessions = std.StringHashMap(Session).init(alloc),
        };
    }

    pub fn deinit(self: *SessionStore) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        var it = self.sessions.iterator();
        while (it.next()) |entry| {
            if (entry.value_ptr.token) |t| self.alloc.free(t);
            self.alloc.free(entry.key_ptr.*);
        }
        self.sessions.deinit();
    }

    pub fn create(self: *SessionStore) ![]const u8 {
        self.mutex.lock();
        defer self.mutex.unlock();

        self.evictExpiredLocked();

        if (self.sessions.count() >= max_sessions) return error.TooManySessions;

        var id_bytes: [12]u8 = undefined;
        std.crypto.random.bytes(&id_bytes);
        const hex = std.fmt.bytesToHex(id_bytes, .lower);
        var session_id: [24]u8 = undefined;
        @memcpy(&session_id, &hex);

        const key = try self.alloc.dupe(u8, &session_id);
        errdefer self.alloc.free(key);

        try self.sessions.put(key, .{
            .session_id = session_id,
            .status = .pending,
            .token = null,
            .created_at_ms = std.time.milliTimestamp(),
        });

        log.info("session created id={s}", .{&session_id});
        return key;
    }

    pub fn poll(self: *SessionStore, session_id: []const u8) PollResult {
        self.mutex.lock();
        defer self.mutex.unlock();

        const entry = self.sessions.get(session_id) orelse {
            log.debug("poll miss id={s}", .{session_id});
            return .{ .status = .expired, .token = null };
        };
        const now = std.time.milliTimestamp();
        if (now - entry.created_at_ms > ttl_ms) {
            log.debug("poll expired id={s}", .{session_id});
            return .{ .status = .expired, .token = null };
        }
        log.debug("poll status={s} id={s}", .{ @tagName(entry.status), session_id });
        return .{ .status = entry.status, .token = entry.token };
    }

    pub fn complete(self: *SessionStore, session_id: []const u8, token: []const u8) !void {
        self.mutex.lock();
        defer self.mutex.unlock();

        const entry = self.sessions.getPtr(session_id) orelse return error.SessionNotFound;
        const now = std.time.milliTimestamp();
        if (now - entry.created_at_ms > ttl_ms) return error.SessionExpired;
        if (entry.status != .pending) return error.SessionAlreadyComplete;

        entry.token = try self.alloc.dupe(u8, token);
        entry.status = .complete;
        log.info("session complete id={s}", .{session_id});
    }

    fn evictExpiredLocked(self: *SessionStore) void {
        const now = std.time.milliTimestamp();
        var to_remove: std.ArrayList([]const u8) = .{};
        defer to_remove.deinit(self.alloc);

        var it = self.sessions.iterator();
        while (it.next()) |entry| {
            if (now - entry.value_ptr.created_at_ms > ttl_ms) {
                to_remove.append(self.alloc, entry.key_ptr.*) catch continue;
            }
        }

        if (to_remove.items.len > 0) {
            log.debug("evicting expired sessions count={d}", .{to_remove.items.len});
        }

        for (to_remove.items) |key| {
            if (self.sessions.fetchRemove(key)) |removed| {
                if (removed.value.token) |t| self.alloc.free(t);
                self.alloc.free(removed.key);
            }
        }
    }
};

pub const PollResult = struct {
    status: SessionStatus,
    token: ?[]const u8,
};

test "create and poll returns pending" {
    var store = SessionStore.init(std.testing.allocator);
    defer store.deinit();

    const id = try store.create();
    const result = store.poll(id);
    try std.testing.expectEqual(SessionStatus.pending, result.status);
    try std.testing.expect(result.token == null);
}

test "complete then poll returns token" {
    var store = SessionStore.init(std.testing.allocator);
    defer store.deinit();

    const id = try store.create();
    try store.complete(id, "jwt_test_token");
    const result = store.poll(id);
    try std.testing.expectEqual(SessionStatus.complete, result.status);
    try std.testing.expectEqualStrings("jwt_test_token", result.token.?);
}

test "poll unknown session returns expired" {
    var store = SessionStore.init(std.testing.allocator);
    defer store.deinit();

    const result = store.poll("nonexistent_session_id_x");
    try std.testing.expectEqual(SessionStatus.expired, result.status);
}

test "double complete returns error" {
    var store = SessionStore.init(std.testing.allocator);
    defer store.deinit();

    const id = try store.create();
    try store.complete(id, "token1");
    try std.testing.expectError(error.SessionAlreadyComplete, store.complete(id, "token2"));
}

test "complete unknown session returns SessionNotFound" {
    var store = SessionStore.init(std.testing.allocator);
    defer store.deinit();

    try std.testing.expectError(error.SessionNotFound, store.complete("nonexistent_id_xxxxxxxxx", "token"));
}

test "complete with empty string token stores it" {
    // Handler validates non-empty; store layer accepts any value
    var store = SessionStore.init(std.testing.allocator);
    defer store.deinit();

    const id = try store.create();
    try store.complete(id, "");
    const result = store.poll(id);
    try std.testing.expectEqual(SessionStatus.complete, result.status);
    try std.testing.expectEqualStrings("", result.token.?);
}

test "session ID is 24 hex characters" {
    var store = SessionStore.init(std.testing.allocator);
    defer store.deinit();

    const id = try store.create();
    try std.testing.expectEqual(@as(usize, 24), id.len);
    for (id) |c| {
        try std.testing.expect((c >= '0' and c <= '9') or (c >= 'a' and c <= 'f'));
    }
}

test "max sessions limit enforced" {
    var store = SessionStore.init(std.testing.allocator);
    defer store.deinit();

    var i: usize = 0;
    while (i < max_sessions) : (i += 1) {
        _ = try store.create();
    }
    try std.testing.expectError(error.TooManySessions, store.create());
}

test "poll with SQL injection payload returns expired gracefully" {
    var store = SessionStore.init(std.testing.allocator);
    defer store.deinit();

    const result = store.poll("'; DROP TABLE sessions;--");
    try std.testing.expectEqual(SessionStatus.expired, result.status);
    try std.testing.expect(result.token == null);
}

test "complete with SQL injection session ID returns SessionNotFound" {
    var store = SessionStore.init(std.testing.allocator);
    defer store.deinit();

    try std.testing.expectError(error.SessionNotFound, store.complete("' OR 1=1; --", "token"));
}

test "poll with empty string returns expired" {
    var store = SessionStore.init(std.testing.allocator);
    defer store.deinit();

    const result = store.poll("");
    try std.testing.expectEqual(SessionStatus.expired, result.status);
}

test "complete with XSS payload in token stores it verbatim" {
    // Store layer is opaque; output encoding is handler/consumer responsibility
    var store = SessionStore.init(std.testing.allocator);
    defer store.deinit();

    const id = try store.create();
    const xss = "<script>alert('xss')</script>";
    try store.complete(id, xss);
    const result = store.poll(id);
    try std.testing.expectEqualStrings(xss, result.token.?);
}

test "multiple sessions are independent" {
    var store = SessionStore.init(std.testing.allocator);
    defer store.deinit();

    const id1 = try store.create();
    const id2 = try store.create();
    try store.complete(id1, "token_a");
    // id2 still pending
    const r1 = store.poll(id1);
    const r2 = store.poll(id2);
    try std.testing.expectEqual(SessionStatus.complete, r1.status);
    try std.testing.expectEqual(SessionStatus.pending, r2.status);
    try std.testing.expectEqualStrings("token_a", r1.token.?);
    try std.testing.expect(r2.token == null);
}
