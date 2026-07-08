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

pub const INSERT =
    "INSERT INTO " ++ TABLE ++
    " (" ++ F_ID ++ SEP ++ F_TENANT_ID ++ SEP ++ F_MODEL_ID ++ SEP ++
    F_SECRET_REF ++ SEP ++ F_CREATED_AT ++ SEP ++ F_UPDATED_AT ++ ") " ++
    "VALUES ($1::uuid, $2::uuid, $3, $4, $5, $5) " ++
    "RETURNING " ++ SELECT_FIELDS;

pub const LIST =
    "SELECT " ++ SELECT_FIELDS ++ " FROM " ++ TABLE ++
    WHERE ++ F_TENANT_ID ++ " = $1::uuid " ++
    "ORDER BY " ++ F_CREATED_AT ++ " DESC, " ++ F_ID ++ " DESC";

pub const UPDATE_MODEL =
    "UPDATE " ++ TABLE ++ " SET " ++ F_MODEL_ID ++ " = $3, " ++
    F_UPDATED_AT ++ " = $4" ++ WHERE ++ MATCH_ID_TENANT ++
    " RETURNING " ++ SELECT_FIELDS;

pub const DELETE =
    "DELETE FROM " ++ TABLE ++ WHERE ++ MATCH_ID_TENANT;

pub const EXISTS_SECRET_IN_PRIMARY_WORKSPACE =
    \\SELECT 1
    \\  FROM vault.secrets s
    \\ WHERE s.workspace_id = (
    \\        SELECT workspace_id
    \\          FROM core.workspaces
    \\         WHERE tenant_id = $1::uuid
    \\         ORDER BY created_at ASC, workspace_id ASC
    \\         LIMIT 1
    \\       )
    \\   AND s.key_name = $2
    \\ LIMIT 1
;

pub const REFERENCED_SECRET_COUNT =
    "SELECT count(*)::bigint FROM " ++ TABLE ++
    WHERE ++ MATCH_TENANT_SECRET;
