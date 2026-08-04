//! Statements for the workspace-create path.
//!
//! Both addressed the retired identity spelling: `core.tenants` keys on `id`,
//! not `tenant_id`, and `core.workspaces` on `id`, not `workspace_id`. Grepping
//! for a renamed column could not see either one — the columns were DROPPED, so
//! these statements named nothing rather than something stale. Both identity
//! columns are UUID and the driver sends text, so the casts are load-bearing.
//!
//! `state/sql.zig` carries a near-twin of INSERT_WORKSPACE that was already
//! correct. They are deliberately not merged: that one ends in
//! `ON CONFLICT ... DO NOTHING`, while this path needs the unique violation to
//! surface so the handler can report a name conflict.

pub const TENANT_EXISTS =
    "SELECT 1 FROM core.tenants WHERE id = $1::uuid LIMIT 1";

pub const INSERT_WORKSPACE =
    \\INSERT INTO core.workspaces
    \\  (id, tenant_id, name, created_by, created_at)
    \\VALUES ($1::uuid, $2::uuid, $3, $4, $5)
;
