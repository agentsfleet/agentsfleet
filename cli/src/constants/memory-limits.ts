// The server's memory defaults, documented in help text — the same values
// RECALL_LIMIT_DEFAULT and LIST_LIMIT_DEFAULT carry in
// rustd/crates/daemon/afd_api_tenant/src/handler/fleet/memory_request/mod.rs.
// The client forwards `limit` only when the operator passed one, so the
// server applies its defaults otherwise, and it never copies the server's
// ceiling: the server applies its own to whatever `limit` arrives.
export const DEFAULT_RECALL_LIMIT = 20;
export const DEFAULT_LIST_LIMIT = 100;
