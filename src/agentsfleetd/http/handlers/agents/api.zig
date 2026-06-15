//! Agent CRUD facade — re-exports the split handler files.
//!
//! The actual handler logic lives in sibling files:
//!   - `create.zig` — POST   /v1/workspaces/{ws}/agents (atomic INSERT + control-stream publish)
//!   - `list.zig`   — GET    /v1/workspaces/{ws}/agents (paginated)
//!   - `patch.zig`  — PATCH  /v1/workspaces/{ws}/agents/{id}
//!                    Body fields (all optional): `config_json`, `status` ∈
//!                    {"active", "stopped", "killed"}. Drives the agent
//!                    status FSM; `paused` is gate-only (set by anomaly
//!                    detection, never by API).
//!   - `delete.zig` — DELETE /v1/workspaces/{ws}/agents/{id}
//!                    Hard-purge. Precondition: status='killed'. Cascades
//!                    every per-agent row across PG schemas + DELs the
//!                    Redis stream.
//!
//! This file exists for backwards compatibility with `route_table_invoke.zig`
//! which imports `agents/api.zig` as a single namespace. New consumers
//! should import the sibling files directly.

const create = @import("create.zig");
const list = @import("list.zig");
const patch = @import("patch.zig");
const delete_h = @import("delete.zig");
const common = @import("../common.zig");

pub const Context = common.Context;

pub const innerCreateAgent = create.innerCreateAgent;
pub const innerListAgents = list.innerListAgents;
pub const innerPatchAgent = patch.innerPatchAgent;
pub const innerDeleteAgent = delete_h.innerDeleteAgent;
