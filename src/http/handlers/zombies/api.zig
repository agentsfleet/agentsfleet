//! Zombie CRUD facade — re-exports the split handler files.
//!
//! The actual handler logic lives in sibling files:
//!   - `create.zig` — POST  /v1/workspaces/{ws}/zombies (atomic INSERT + control-stream publish)
//!   - `list.zig`   — GET   /v1/workspaces/{ws}/zombies (paginated)
//!   - `kill.zig`   — POST  /v1/workspaces/{ws}/zombies/{id}/kill (status=killed + control-stream publish)
//!   - `patch.zig`  — PATCH /v1/workspaces/{ws}/zombies/{id} (config_json + control-stream publish)
//!
//! This file exists for backwards compatibility with `route_table_invoke.zig`
//! which imports `zombies/api.zig` as a single namespace. New consumers
//! should import the sibling files directly.
//!
//! The legacy DELETE /v1/workspaces/{ws}/zombies/{id} handler was removed —
//! POST /kill replaces it cleanly. Pre-v2.0, no 410 stub.

const create = @import("create.zig");
const list = @import("list.zig");
const kill = @import("kill.zig");
const patch = @import("patch.zig");
const common = @import("../common.zig");

pub const Context = common.Context;

pub const innerCreateZombie = create.innerCreateZombie;
pub const innerListZombies = list.innerListZombies;
pub const innerKillZombie = kill.innerKillZombie;
pub const innerPatchZombie = patch.innerPatchZombie;
