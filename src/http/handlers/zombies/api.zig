//! Zombie CRUD facade — re-exports the split handler files.
//!
//! The actual handler logic lives in sibling files:
//!   - `create.zig` — POST /v1/workspaces/{ws}/zombies (atomic INSERT + control-stream publish)
//!   - `list.zig`   — GET  /v1/workspaces/{ws}/zombies (paginated)
//!   - `delete.zig` — DELETE /v1/workspaces/{ws}/zombies/{id} (legacy kill verb; slated for removal)
//!
//! This file exists for backwards compatibility with `route_table_invoke.zig`
//! which imports `zombies/api.zig` as a single namespace. New consumers
//! should import the sibling files directly.

const create = @import("create.zig");
const list = @import("list.zig");
const delete = @import("delete.zig");
const common = @import("../common.zig");

pub const Context = common.Context;

pub const innerCreateZombie = create.innerCreateZombie;
pub const innerListZombies = list.innerListZombies;
pub const innerDeleteZombie = delete.innerDeleteZombie;
