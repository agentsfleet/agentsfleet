//! Centralised SQL for the model library catalogue (core.model_library).
//! Every production query against the table lives here so the table name is
//! grepable from one place and the store module stays focused on row mapping,
//! allocator ownership, and error translation (tests keep their setup/teardown
//! SQL inline per the SQL Statement Modules rule). Mirrors state/tenant_model_entries/sql.zig.

/// The catalogue table — single source for every core.model_library reference.
pub const TABLE = "core.model_library";

const RATE_COLUMNS =
    "context_cap_tokens, input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok";
const FROM_TABLE = "\n  FROM " ++ TABLE;

/// Every catalogue row for the admin list, ordered by the (provider, model_id) identity.
pub const LIST_ADMIN =
    "SELECT uid::text, provider, model_id, " ++ RATE_COLUMNS ++
    FROM_TABLE ++
    "\n ORDER BY provider, model_id";

/// The catalogue as the authenticated library read serves it, plus updated_at_ms
/// per row (the max drives the response's version stamp).
pub const LIST_LIBRARY =
    "SELECT model_id, provider, " ++ RATE_COLUMNS ++ ", updated_at_ms" ++
    FROM_TABLE ++
    "\n ORDER BY model_id";

/// context_cap_tokens of one priced (provider, model_id) row — the
/// platform-default PUT snapshots the cap through this.
pub const CAP_FOR =
    "SELECT context_cap_tokens FROM " ++ TABLE ++ " WHERE provider = $1 AND model_id = $2 LIMIT 1";

/// True-row probe: is the uid the (provider, model) the active
/// platform_provider_defaults row resolves to? (The delete-guard.)
pub const IS_REFERENCED_BY_ACTIVE_DEFAULT =
    "SELECT 1\n  FROM " ++ TABLE ++ " mc" ++
    \\
    \\  JOIN core.platform_provider_defaults plk
    \\    ON plk.provider = mc.provider AND plk.model = mc.model_id AND plk.active = true
    \\ WHERE mc.uid = $1::uuid
    \\ LIMIT 1
;

/// Insert one priced row; ON CONFLICT (provider, model_id) DO NOTHING so the
/// affected count is 1 on create and 0 on a duplicate (caller → 409).
pub const INSERT_ROW =
    "INSERT INTO " ++ TABLE ++
    \\
    \\  (uid, model_id, provider, context_cap_tokens,
    \\   input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok,
    \\   created_at_ms, updated_at_ms)
    \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $8)
    \\ON CONFLICT (provider, model_id) DO NOTHING
;

/// Update caps/rates of the row identified by uid. Affected 0 → no such uid
/// (caller → 404).
pub const UPDATE_RATES =
    "UPDATE " ++ TABLE ++
    \\
    \\   SET context_cap_tokens = $2, input_nanos_per_mtok = $3,
    \\       cached_input_nanos_per_mtok = $4, output_nanos_per_mtok = $5,
    \\       updated_at_ms = $6
    \\ WHERE uid = $1::uuid
;

/// Delete the row identified by uid. Affected 0 → no such uid (caller → 404).
pub const DELETE_BY_UID =
    "DELETE FROM " ++ TABLE ++ " WHERE uid = $1::uuid";

/// Full rate projection for the in-memory rate cache (model_rate_cache.zig) —
/// keyed by (provider, model_id) at load time. Column order follows
/// RATE_COLUMNS (cap first), matching the cache populator's indices.
pub const LIST_RATES_FOR_CACHE =
    "SELECT provider, model_id, " ++ RATE_COLUMNS ++ FROM_TABLE;
