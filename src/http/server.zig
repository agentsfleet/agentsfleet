//! Zap HTTP server setup and request routing.
//! Thread 1 — all endpoint handlers run here. Never blocks on agent execution.

const std = @import("std");
const zap = @import("zap");
const handler = @import("handler.zig");
const log = std.log.scoped(.http);

pub const ServerConfig = struct {
    port: u16 = 3000,
    interface: []const u8 = "0.0.0.0",
    threads: i16 = 1,
    workers: i16 = 1,
    max_clients: usize = 1024,
};

/// Single global context pointer used by the Zap callbacks.
/// Zap's C event loop doesn't support closures, so we use a module-level var.
var g_ctx: *handler.Context = undefined;

// ── Request dispatch ──────────────────────────────────────────────────────

// Route prefixes used in 3+ dispatch branches.
const prefix_workspaces = "/v1/workspaces/";
const prefix_runs = "/v1/runs/";
const prefix_auth_sessions = "/v1/auth/sessions/";

/// Top-level request handler — dispatches based on method + path prefix.
fn dispatch(r: zap.Request) !void {
    const path = r.path orelse {
        r.setStatus(.bad_request);
        r.sendBody("") catch {};
        return;
    };

    // Health check (no auth)
    if (std.mem.eql(u8, path, "/healthz")) {
        handler.handleHealthz(g_ctx, r);
        return;
    }
    if (std.mem.eql(u8, path, "/readyz")) {
        handler.handleReadyz(g_ctx, r);
        return;
    }
    if (std.mem.eql(u8, path, "/metrics")) {
        handler.handleMetrics(g_ctx, r);
        return;
    }

    // Route: /v1/auth/sessions (create)
    if (std.mem.eql(u8, path, "/v1/auth/sessions")) {
        if (r.methodAsEnum() == .POST) {
            handler.handleCreateAuthSession(g_ctx, r);
        } else {
            r.setStatus(.method_not_allowed);
            r.sendBody("") catch {};
        }
        return;
    }

    // Route: /v1/auth/sessions/:id/complete
    if (std.mem.startsWith(u8, path, prefix_auth_sessions) and
        std.mem.endsWith(u8, path, "/complete"))
    {
        const inner = path[prefix_auth_sessions.len .. path.len - "/complete".len];
        if (inner.len > 0 and std.mem.indexOfScalar(u8, inner, '/') == null) {
            if (r.methodAsEnum() == .POST) {
                handler.handleCompleteAuthSession(g_ctx, r, inner);
            } else {
                r.setStatus(.method_not_allowed);
                r.sendBody("") catch {};
            }
            return;
        }
    }

    // Route: /v1/auth/sessions/:id (poll)
    if (std.mem.startsWith(u8, path, prefix_auth_sessions)) {
        const session_id = path[prefix_auth_sessions.len..];
        if (session_id.len > 0 and !std.mem.containsAtLeast(u8, session_id, 1, "/")) {
            if (r.methodAsEnum() == .GET) {
                handler.handlePollAuthSession(g_ctx, r, session_id);
            } else {
                r.setStatus(.method_not_allowed);
                r.sendBody("") catch {};
            }
            return;
        }
    }

    // Route: /v1/github/callback
    if (std.mem.eql(u8, path, "/v1/github/callback")) {
        if (r.methodAsEnum() == .GET) {
            handler.handleGitHubCallback(g_ctx, r);
        } else {
            r.setStatus(.method_not_allowed);
            r.sendBody("") catch {};
        }
        return;
    }

    // Route: /v1/workspaces (create)
    if (std.mem.eql(u8, path, "/v1/workspaces")) {
        if (r.methodAsEnum() == .POST) {
            handler.handleCreateWorkspace(g_ctx, r);
        } else {
            r.setStatus(.method_not_allowed);
            r.sendBody("") catch {};
        }
        return;
    }

    // Route: /v1/runs
    if (std.mem.eql(u8, path, "/v1/runs")) {
        if (r.methodAsEnum() == .POST) {
            handler.handleStartRun(g_ctx, r);
        } else {
            r.setStatus(.method_not_allowed);
            r.sendBody("") catch {};
        }
        return;
    }

    // Route: /v1/specs
    if (std.mem.eql(u8, path, "/v1/specs")) {
        if (r.methodAsEnum() == .GET) {
            handler.handleListSpecs(g_ctx, r);
        } else {
            r.setStatus(.method_not_allowed);
            r.sendBody("") catch {};
        }
        return;
    }

    // Route: /v1/runs/:run_id:retry
    if (std.mem.startsWith(u8, path, prefix_runs) and
        std.mem.endsWith(u8, path, ":retry"))
    {
        if (r.methodAsEnum() == .POST) {
            const inner = path[prefix_runs.len .. path.len - ":retry".len];
            handler.handleRetryRun(g_ctx, r, inner);
        } else {
            r.setStatus(.method_not_allowed);
            r.sendBody("") catch {};
        }
        return;
    }

    // Route: /v1/runs/:run_id
    if (std.mem.startsWith(u8, path, prefix_runs)) {
        const run_id = path[prefix_runs.len..];
        if (run_id.len > 0 and !std.mem.containsAtLeast(u8, run_id, 1, "/")) {
            if (r.methodAsEnum() == .GET) {
                handler.handleGetRun(g_ctx, r, run_id);
            } else {
                r.setStatus(.method_not_allowed);
                r.sendBody("") catch {};
            }
            return;
        }
    }

    // Route: /v1/workspaces/:workspace_id:pause
    if (std.mem.startsWith(u8, path, prefix_workspaces) and
        std.mem.endsWith(u8, path, ":pause"))
    {
        if (r.methodAsEnum() == .POST) {
            const inner = path[prefix_workspaces.len .. path.len - ":pause".len];
            handler.handlePauseWorkspace(g_ctx, r, inner);
        } else {
            r.setStatus(.method_not_allowed);
            r.sendBody("") catch {};
        }
        return;
    }

    // Route: /v1/workspaces/{workspace_id}/harness/source
    if (std.mem.startsWith(u8, path, prefix_workspaces) and
        std.mem.endsWith(u8, path, "/harness/source"))
    {
        if (r.methodAsEnum() == .PUT) {
            const inner = path[prefix_workspaces.len .. path.len - "/harness/source".len];
            if (inner.len > 0 and std.mem.indexOfScalar(u8, inner, '/') == null) {
                handler.handlePutHarnessSource(g_ctx, r, inner);
                return;
            }
        } else {
            r.setStatus(.method_not_allowed);
            r.sendBody("") catch {};
            return;
        }
    }

    // Route: /v1/workspaces/{workspace_id}/harness/compile
    if (std.mem.startsWith(u8, path, prefix_workspaces) and
        std.mem.endsWith(u8, path, "/harness/compile"))
    {
        if (r.methodAsEnum() == .POST) {
            const inner = path[prefix_workspaces.len .. path.len - "/harness/compile".len];
            if (inner.len > 0 and std.mem.indexOfScalar(u8, inner, '/') == null) {
                handler.handleCompileHarness(g_ctx, r, inner);
                return;
            }
        } else {
            r.setStatus(.method_not_allowed);
            r.sendBody("") catch {};
            return;
        }
    }

    // Route: /v1/workspaces/{workspace_id}/harness/activate
    if (std.mem.startsWith(u8, path, prefix_workspaces) and
        std.mem.endsWith(u8, path, "/harness/activate"))
    {
        if (r.methodAsEnum() == .POST) {
            const inner = path[prefix_workspaces.len .. path.len - "/harness/activate".len];
            if (inner.len > 0 and std.mem.indexOfScalar(u8, inner, '/') == null) {
                handler.handleActivateHarness(g_ctx, r, inner);
                return;
            }
        } else {
            r.setStatus(.method_not_allowed);
            r.sendBody("") catch {};
            return;
        }
    }

    // Route: /v1/workspaces/{workspace_id}/harness/active
    if (std.mem.startsWith(u8, path, prefix_workspaces) and
        std.mem.endsWith(u8, path, "/harness/active"))
    {
        if (r.methodAsEnum() == .GET) {
            const inner = path[prefix_workspaces.len .. path.len - "/harness/active".len];
            if (inner.len > 0 and std.mem.indexOfScalar(u8, inner, '/') == null) {
                handler.handleGetHarnessActive(g_ctx, r, inner);
                return;
            }
        } else {
            r.setStatus(.method_not_allowed);
            r.sendBody("") catch {};
            return;
        }
    }

    // Route: /v1/workspaces/{workspace_id}/skills/{skill_ref}/secrets/{key_name}
    if (handler.parseSkillSecretRoute(path)) |route| {
        switch (r.methodAsEnum()) {
            .PUT => handler.handlePutWorkspaceSkillSecret(g_ctx, r, route.workspace_id, route.skill_ref_encoded, route.key_name_encoded),
            .DELETE => handler.handleDeleteWorkspaceSkillSecret(g_ctx, r, route.workspace_id, route.skill_ref_encoded, route.key_name_encoded),
            else => {
                r.setStatus(.method_not_allowed);
                r.sendBody("") catch {};
            },
        }
        return;
    }

    // Route: /v1/workspaces/:workspace_id:sync
    if (std.mem.startsWith(u8, path, prefix_workspaces) and
        std.mem.endsWith(u8, path, ":sync"))
    {
        if (r.methodAsEnum() == .POST) {
            const inner = path[prefix_workspaces.len .. path.len - ":sync".len];
            handler.handleSyncSpecs(g_ctx, r, inner);
        } else {
            r.setStatus(.method_not_allowed);
            r.sendBody("") catch {};
        }
        return;
    }

    r.setStatus(.not_found);
    r.sendBody(
        \\{"error":{"code":"NOT_FOUND","message":"No such route"}}
    ) catch {};
}

// ── Server lifecycle ──────────────────────────────────────────────────────

/// Start the Zap HTTP server. Blocks until zap.stop() is called.
pub fn serve(ctx: *handler.Context, cfg: ServerConfig) !void {
    g_ctx = ctx;

    var listener = zap.HttpListener.init(.{
        .port = cfg.port,
        .on_request = dispatch,
        .log = false,
        .max_clients = cfg.max_clients,
        .max_body_size = 2 * 1024 * 1024, // 2MB
    });
    try listener.listen();

    log.info("listening on 0.0.0.0:{d}", .{cfg.port});

    zap.start(.{
        .threads = cfg.threads,
        .workers = cfg.workers,
    });
}

pub fn stop() void {
    zap.stop();
}
