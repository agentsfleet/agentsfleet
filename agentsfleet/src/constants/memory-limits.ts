// Mirrors of the server's published memory limit constants — same
// identifiers as src/agentsfleetd/http/handlers/memory/helpers.zig and the
// OpenAPI bounds on list_zombie_memories (RULE UFS: cross-runtime constants
// share a name). The client validates against the cap and documents the
// defaults in help text; it never invents its own caps, and it only
// forwards `limit` when the operator passed one (the server applies its
// defaults otherwise).
export const MAX_RECALL_LIMIT = 100;
export const DEFAULT_RECALL_LIMIT = 20;
export const DEFAULT_LIST_LIMIT = 100;
