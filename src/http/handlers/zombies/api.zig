//! Zombie CRUD facade — re-exports the split handler files.
//!
//! The actual handler logic lives in sibling files:
//!   - `create.zig` — POST  /v1/workspaces/{ws}/zombies (atomic INSERT + control-stream publish)
//!   - `list.zig`   — GET   /v1/workspaces/{ws}/zombies (paginated)
//!   - `patch.zig`  — PATCH /v1/workspaces/{ws}/zombies/{id}
//!                   Body fields (all optional): config_json, status:"killed".
//!                   The kill flow (status=killed + control-stream publish)
//!                   folds into this handler; presence-based dispatch.
//!
//! This file exists for backwards compatibility with `route_table_invoke.zig`
//! which imports `zombies/api.zig` as a single namespace. New consumers
//! should import the sibling files directly.

const create = @import("create.zig");
const list = @import("list.zig");
const patch = @import("patch.zig");
const common = @import("../common.zig");

pub const Context = common.Context;

pub const innerCreateZombie = create.innerCreateZombie;
pub const innerListZombies = list.innerListZombies;
pub const innerPatchZombie = patch.innerPatchZombie;
