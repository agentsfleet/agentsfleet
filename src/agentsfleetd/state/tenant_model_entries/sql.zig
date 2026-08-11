//! Centralised SQL for tenant model registry entries (M121).
//! Every query against `core.tenant_model_entries` lives here so the table name
//! is grepable from one file and the state module stays focused on ownership.

const TABLE = "core.tenant_model_entries";
const F_ID = "id";
const F_TENANT_ID = "tenant_id";
const F_MODEL_ID = "model_id";
const F_SECRET_REF = "secret_ref";
const F_CREATED_AT = "created_at";
const F_UPDATED_AT = "updated_at";
const SEP = ", ";
const TEXT_SEP = "::text" ++ SEP;
const WHERE = " WHERE ";
const PARAM1_UUID_AND = " = $1::uuid AND ";
const MATCH_ID_TENANT =
    F_ID ++ PARAM1_UUID_AND ++ F_TENANT_ID ++ " = $2::uuid";
const MATCH_TENANT_SECRET =
    F_TENANT_ID ++ PARAM1_UUID_AND ++ F_SECRET_REF ++ " = $2";

const SELECT_FIELDS =
    F_ID ++ TEXT_SEP ++ F_TENANT_ID ++ TEXT_SEP ++ F_MODEL_ID ++ SEP ++
    F_SECRET_REF ++ SEP ++ F_CREATED_AT ++ SEP ++ F_UPDATED_AT;

// Shared INSERT prefix — both write statements below (RETURNING vs
// ON CONFLICT) share the same table, column list, and VALUES clause.
const ENTRY_TENANT_SECRET_TUPLE = F_TENANT_ID ++ SEP ++ F_MODEL_ID ++ SEP ++ F_SECRET_REF;
const INSERT_PREFIX =
    "INSERT INTO " ++ TABLE ++
    " (" ++ F_ID ++ SEP ++ ENTRY_TENANT_SECRET_TUPLE ++ SEP ++ F_CREATED_AT ++ SEP ++ F_UPDATED_AT ++ ") " ++
    "VALUES ($1::uuid, $2::uuid, $3, $4, $5, $5) ";

pub const INSERT = INSERT_PREFIX ++ "RETURNING " ++ SELECT_FIELDS;

// ON CONFLICT DO NOTHING (not raise-and-catch): a duplicate is the COMMON
// case on re-activation, and a clean no-op costs one round-trip with no
// unique-violation error or aborted subtransaction.
pub const INSERT_IF_ABSENT =
    INSERT_PREFIX ++ "ON CONFLICT (" ++ ENTRY_TENANT_SECRET_TUPLE ++ ") DO NOTHING";

/// `SELECT <fields> FROM <table> WHERE tenant_id = $1` — the opening every
/// tenant-scoped read shares. One spelling, so the projection and the tenant
/// predicate cannot drift between the full list and the two page statements.
/// `SELECT <fields> FROM <table>` — the projection every read shares, named so
/// the column list and the table cannot drift between the tenant-scoped reads
/// and the by-id read.
const SELECT_PROJECTION = "SELECT " ++ SELECT_FIELDS ++ " FROM " ++ TABLE;

const SELECT_FOR_TENANT =
    SELECT_PROJECTION ++ WHERE ++ F_TENANT_ID ++ " = $1::uuid";

pub const LIST = SELECT_FOR_TENANT ++ " " ++ ORDER_BY_KEYSET;

/// The page order, shared by both page statements so they cannot drift.
///
/// The spec writes this key as `created_at DESC, id COLLATE "C" DESC`. `id` is
/// a `UUID` column, and collations apply to text, so honouring that literally
/// would mean `id::text COLLATE "C"` — which casts every row and makes
/// idx_tenant_model_entries_tenant_created_at unusable for the sort. For
/// canonical lowercase UUIDv7 values the two orders are identical (the hex
/// alphabet sorts the same by byte and by ASCII), so the native uuid ordering
/// is used: same sequence, index intact.
const ORDER_BY_KEYSET =
    "ORDER BY " ++ F_CREATED_AT ++ " DESC, " ++ F_ID ++ " DESC";

/// First page of the tenant registry keyset. `$2` is `limit + 1`: the extra row
/// is never returned, it only answers "is there another page?" without a second
/// COUNT query.
pub const LIST_PAGE_FIRST =
    SELECT_FOR_TENANT ++ " " ++ ORDER_BY_KEYSET ++ " LIMIT $2";

/// Subsequent pages. The row-wise comparison `(created_at, id) < ($2, $3)` is
/// the seek predicate, and it is deliberately written as a row comparison rather
/// than the expanded `created_at < $2 OR (created_at = $2 AND id < $3)`: the two
/// are equivalent, but only the row form is matched to the composite index as a
/// single range start, and the expanded form is where off-by-one bugs live when
/// someone later edits one arm.
///
/// A boundary row deleted between pages is not a problem — the comparison is
/// against VALUES, not against a row that must still exist. That is what makes a
/// stale cursor continue rather than error.
pub const LIST_PAGE_AFTER =
    SELECT_FOR_TENANT ++
    " AND (" ++ F_CREATED_AT ++ SEP ++ F_ID ++ ") < ($2::bigint, $3::uuid) " ++
    ORDER_BY_KEYSET ++ " LIMIT $4";

pub const UPDATE_MODEL =
    "UPDATE " ++ TABLE ++ " SET " ++ F_MODEL_ID ++ " = $3, " ++
    F_UPDATED_AT ++ " = $4" ++ WHERE ++ MATCH_ID_TENANT ++
    " RETURNING " ++ SELECT_FIELDS;

pub const DELETE =
    "DELETE FROM " ++ TABLE ++ WHERE ++ MATCH_ID_TENANT;

/// One entry by id. The delete path needs the row's `secret_ref` BEFORE it can
/// open the reference transaction — the shared lock order is keyed on the
/// credential, so the credential has to be named first.
pub const SELECT_BY_ID = SELECT_PROJECTION ++ WHERE ++ MATCH_ID_TENANT;

/// Split in two: the workspace resolution reads `core.*` as `api_runtime`,
/// while the secret probe needs `vault_runtime` (schema/300) — `SET ROLE`
/// replaces the privilege set, so one statement cannot span both.
pub const SELECT_PRIMARY_WORKSPACE =
    \\SELECT id::text
    \\  FROM core.workspaces
    \\ WHERE tenant_id = $1::uuid
    \\ ORDER BY created_at ASC, id ASC
    \\ LIMIT 1
;

pub const EXISTS_SECRET_IN_WORKSPACE =
    \\SELECT 1
    \\  FROM vault.secrets s
    \\ WHERE s.workspace_id = $1::uuid
    \\   AND s.key_name = $2
    \\ LIMIT 1
;

pub const REFERENCED_SECRET_COUNT =
    "SELECT count(*)::bigint FROM " ++ TABLE ++
    WHERE ++ MATCH_TENANT_SECRET;
