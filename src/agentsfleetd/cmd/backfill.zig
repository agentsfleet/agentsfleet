//! `agentsfleetd backfill` — fill the non-secret projection on credentials
//! stored before `schema/036_vault_secret_metadata.sql`.
//!
//! A one-shot operator command, not part of `serve` or `migrate`. It is
//! separate from `migrate` on purpose: the projection is derived from the
//! DECRYPTED credential body, and the Key Encryption Key lives in the
//! application, so a schema migration can only add the columns as NULL. This
//! command is the half that needs the key.
//!
//! Run it once per database after `migrate`. It is idempotent — it selects only
//! workspaces that still hold an unprojected row — so a repeat run or a resume
//! after failure is safe.
//!
//! The sweep is all this process does, so like `migrate` it skips the
//! OpenTelemetry Protocol exporter: the command finishes well inside the
//! exporter's flush interval, and its shutdown drain is what hung release
//! commands on a deploy host. Logs still reach the operator through stdout.

const std = @import("std");
const constants = @import("common");
const logging = @import("log");

const db = @import("../db/pool.zig");
const error_codes = @import("../errors/error_registry.zig");
const metadata_backfill = @import("../secrets/metadata_backfill.zig");

const log = logging.scoped(.agentsfleetd);

const EnvMap = constants.env.Map;

pub fn run(io: std.Io, env_map: *const EnvMap, alloc: std.mem.Allocator) !void {
    // The `api` role, not `migrator`: this writes rows, not schema, and
    // schema/002 already grants api_runtime UPDATE on vault.secrets. Reaching
    // for the migrator role would hand a data sweep full authority over Data
    // Definition Language for no reason.
    const pool = db.initFromEnvForRole(io, env_map, alloc, .api) catch |err| {
        log.err("backfill.db_connect_failed", .{
            .error_code = error_codes.ERR_STARTUP_DB_CONNECT,
            .err = @errorName(err),
        });
        std.process.exit(1);
    };
    defer pool.deinit();

    const conn = pool.acquire() catch |err| {
        log.err("backfill.conn_acquire_failed", .{
            .error_code = error_codes.ERR_STARTUP_DB_CONNECT,
            .err = @errorName(err),
        });
        std.process.exit(1);
    };
    defer pool.release(conn);

    const stats = metadata_backfill.run(alloc, conn) catch |err| {
        log.err("backfill.failed", .{
            .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED,
            .err = @errorName(err),
        });
        std.process.exit(1);
    };

    // Counts only — never a key name, provider, or endpoint. Credential
    // metadata stays out of logs even though it is not itself secret.
    log.info("backfill.summary", .{
        .workspaces = stats.workspaces,
        .projected = stats.projected,
        .undecryptable = stats.undecryptable,
        .opaque_bodies = stats.opaque_bodies,
        .rotated_midway = stats.rotated_midway,
    });

    // Undecryptable rows are reported, not fatal: they keep reporting as opaque
    // credentials, which is truthful, and failing the whole sweep over one
    // damaged envelope would block every healthy row behind it.
    if (stats.undecryptable > 0) {
        log.warn("backfill.undecryptable_rows_remain", .{ .count = stats.undecryptable });
    }
}
