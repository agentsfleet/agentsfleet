//! Fleet CRUD facade — re-exports the split handler files.
//!
//! The actual handler logic lives in sibling files:
//!   - `create.zig` — POST   /v1/workspaces/{ws}/fleets (atomic INSERT + control-stream publish)
//!   - `list.zig`   — GET    /v1/workspaces/{ws}/fleets (paginated)
//!   - `patch.zig`  — PATCH  /v1/workspaces/{ws}/fleets/{id}
//!                    Body fields (all optional): `config_json`, `status` ∈
//!                    {"active", "stopped", "killed"}. Drives the fleet
//!                    status FSM; `paused` is gate-only (set by anomaly
//!                    detection, never by API).
//!   - `delete.zig` — DELETE /v1/workspaces/{ws}/fleets/{id}
//!                    Hard-purge. Precondition: status='killed'. Cascades
//!                    every per-fleet row across PG schemas + DELs the
//!                    Redis stream.
//!
//! This file exists for backwards compatibility with `route_table_invoke.zig`
//! which imports `fleets/api.zig` as a single namespace. New consumers
//! should import the sibling files directly.

const create = @import("create.zig");
const list = @import("list.zig");
const patch = @import("patch.zig");
const delete_h = @import("delete.zig");
const common = @import("../common.zig");

pub const Context = common.Context;

pub const innerCreateFleet = create.innerCreateFleet;
pub const innerListFleets = list.innerListFleets;
pub const innerPatchFleet = patch.innerPatchFleet;
pub const innerDeleteFleet = delete_h.innerDeleteFleet;
