//! Leaf module — types shared between `pool.zig` (the connection pool
//! surface) and `pool_migrations.zig` (the versioned schema runner).
//!
//! Lives at the bottom of the import graph to break the cycle that
//! existed when both files imported each other for these definitions.
//! Neither this file nor anything it imports may reach back into
//! `pool.zig` or `pool_migrations.zig`.

pub const Migration = struct {
    version: i32,
    sql: []const u8,
};

pub const MigrationState = struct {
    expected_versions: u32,
    applied_versions: u32,
    latest_expected_version: i32,
    latest_applied_version: i32,
    has_failed_migrations: bool,
    lock_available: bool,
    has_newer_schema_version: bool,
};
