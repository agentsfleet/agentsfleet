// Mirrors of the server's published memory limit constants — the same
// values LIMIT_MAX, RECALL_LIMIT_DEFAULT and LIST_LIMIT_DEFAULT carry in
// rustd/crates/afd_api_tenant/src/handler/fleet/memory_request.rs, and the
// OpenAPI bounds on list_fleet_memories (RULE UFS: a cross-runtime constant
// is one fact). The client validates against the cap and documents the
// defaults in help text; it never invents its own caps, and it only
// forwards `limit` when the operator passed one (the server applies its
// defaults otherwise).
export const MAX_RECALL_LIMIT = 100;
export const DEFAULT_RECALL_LIMIT = 20;
export const DEFAULT_LIST_LIMIT = 100;
