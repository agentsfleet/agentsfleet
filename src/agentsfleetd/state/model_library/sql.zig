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

/// The wire columns every authenticated library read projects, shared by the
/// unpaged list and the §2 page so their column indices cannot drift apart.
const SELECT_LIBRARY_HEAD = "SELECT model_id, provider, ";

/// Every catalogue row for the admin list, ordered by the (provider, model_id) identity.
pub const LIST_ADMIN =
    "SELECT uid::text, provider, model_id, " ++ RATE_COLUMNS ++
    FROM_TABLE ++
    "\n ORDER BY provider, model_id";

/// The catalogue as the authenticated library read serves it, plus updated_at_ms
/// per row (the max drives the response's version stamp).
pub const LIST_LIBRARY =
    SELECT_LIBRARY_HEAD ++ RATE_COLUMNS ++ ", updated_at_ms" ++
    FROM_TABLE ++
    "\n ORDER BY model_id";

// ── The bounded catalogue page (§2) ─────────────────────────────────────────
//
// Normalization is SQL-side, not handler-side: Zig's standard library ships no
// Unicode tables, while Postgres `normalize(text, NFKC)` is built in and
// IMMUTABLE. Folding BOTH the column and the needle with the same expression is
// what makes a match independent of the script the caller typed in — a
// half-folded needle against a fully folded column matches by accident.
// `handlers/library/query.zig` keeps only the ASCII-safe half.

/// The two sort/search keys, spelled once so the ORDER BY, the seek predicate
/// and the projection cannot fold differently from each other.
const DISPLAY_KEY = "lower(normalize(model_id, NFKC))";
const VENDOR_KEY = "lower(normalize(provider, NFKC))";

/// Paired with every LIKE built by `query.likeContains`, whose escape character
/// this must match or the escaping there is inert.
const LIKE_ESCAPE_CLAUSE =
    \\ ESCAPE '\'
;

/// The search needle, folded by the same expression as the columns it is
/// compared against. Spelled once because the two OR branches below must fold
/// identically — one of them folding differently is a match that depends on
/// whether the hit came from the display or the vendor column.
const LIKE_FOLDED_QUERY = " LIKE lower(normalize($1, NFKC))" ++ LIKE_ESCAPE_CLAUSE;

/// Byte-order collation, as a key separator. Every sort key and every seek
/// operand carries it, so the ORDER BY and the predicate that resumes it cannot
/// compare under different rules.
const COLLATE_C_SEP = " COLLATE \"C\", ";
const KEY_INDENT = "\n       ";
const OR_INDENT = "\n        OR ";

/// The page order.
///
/// The tiebreak is `uid`, NOT `(provider, model_id)`. Normalization is
/// many-to-one, so two rows distinct in the table can share a display/vendor key
/// pair after folding — a key that is unique before normalization is not unique
/// in this sort, and a keyset whose last component can repeat either skips rows
/// or loops on them.
///
/// `uid` is ordered natively rather than as `uid::text COLLATE "C"`: for
/// canonical lowercase UUIDs the two sequences are identical (the hex alphabet
/// sorts the same by byte and by ASCII), and the cast would force a per-row
/// conversion. Same reasoning as `tenant_model_entries/sql.zig`.
const ORDER_BY_LIBRARY_KEYSET =
    "\n ORDER BY " ++ DISPLAY_KEY ++ COLLATE_C_SEP ++ VENDOR_KEY ++ COLLATE_C_SEP ++ "uid";

/// The page projection: the wire columns, plus the two normalized keys and the
/// uid the next cursor is built from. `uid` never reaches `LibraryRow` — it is
/// the sort tiebreak and rides the cursor opaquely, nothing more.
const SELECT_LIBRARY_PAGE =
    SELECT_LIBRARY_HEAD ++ RATE_COLUMNS ++ ", updated_at_ms, uid::text," ++
    KEY_INDENT ++ DISPLAY_KEY ++ " AS display_key," ++
    KEY_INDENT ++ VENDOR_KEY ++ " AS vendor_key" ++
    FROM_TABLE;

/// `$1` = the LIKE pattern (null ⇒ no search), `$2` = the provider (null ⇒ no
/// filter). Both needles are folded by the same expression as the columns.
const WHERE_LIBRARY_FILTERS =
    "\n WHERE ($1::text IS NULL" ++
    OR_INDENT ++ DISPLAY_KEY ++ LIKE_FOLDED_QUERY ++
    OR_INDENT ++ VENDOR_KEY ++ LIKE_FOLDED_QUERY ++ ")" ++
    "\n   AND ($2::text IS NULL OR " ++ VENDOR_KEY ++ " = lower(normalize($2, NFKC)))";

/// First page. `$3` is `limit + 1`: the extra row never reaches the response, it
/// only answers "is there another page?" without a second COUNT.
pub const LIST_LIBRARY_PAGE_FIRST =
    SELECT_LIBRARY_PAGE ++ WHERE_LIBRARY_FILTERS ++ ORDER_BY_LIBRARY_KEYSET ++ "\n LIMIT $3";

/// Resume after a cursor. The row-wise comparison is the seek predicate: strictly
/// greater on the first key that differs, which is exactly the order above.
pub const LIST_LIBRARY_PAGE_AFTER =
    SELECT_LIBRARY_PAGE ++ WHERE_LIBRARY_FILTERS ++
    "\n   AND (" ++ DISPLAY_KEY ++ COLLATE_C_SEP ++ VENDOR_KEY ++ COLLATE_C_SEP ++ "uid)" ++
    "\n     > ($3::text" ++ COLLATE_C_SEP ++ "$4::text" ++ COLLATE_C_SEP ++ "$5::uuid)" ++
    ORDER_BY_LIBRARY_KEYSET ++ "\n LIMIT $6";

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
