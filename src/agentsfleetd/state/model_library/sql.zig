//! Centralised SQL for the model library catalogue — `core.model_library` and
//! the generation counter that versions it, `core.model_catalogue_revision`.
//! Every production query against either table lives here so the table names are
//! grepable from one place and the store modules stay focused on row mapping,
//! allocator ownership, and error translation (tests keep their setup/teardown
//! SQL inline per the SQL Statement Modules rule). Mirrors state/tenant_model_entries/sql.zig.
//!
//! The two tables share a module because they are one domain and one read: the
//! rate loader below projects a row and the generation it was read at in a single
//! statement, which needs both names. Splitting them would put half of that
//! statement in another file.

/// The catalogue table — single source for every core.model_library reference.
pub const TABLE = "core.model_library";

/// The generation table — single source for every core.model_catalogue_revision
/// reference.
pub const REVISION_TABLE = "core.model_catalogue_revision";

/// The four rate columns, optionally table-qualified.
///
/// A joined read cannot reuse the unqualified list: `core.model_library` and
/// `core.model_catalogue_revision` both carry `updated_at_ms`, so an unqualified
/// projection across the two is one added column away from an ambiguity error.
/// Generating both spellings from one source keeps the qualified variant from
/// drifting when a rate column is added.
fn rateColumns(comptime prefix: []const u8) []const u8 {
    return prefix ++ "context_cap_tokens, " ++
        prefix ++ "input_nanos_per_mtok, " ++
        prefix ++ "cached_input_nanos_per_mtok, " ++
        prefix ++ "output_nanos_per_mtok";
}

const RATE_COLUMNS = rateColumns("");
const RATE_COLUMNS_JOINED = rateColumns("m.");
/// `FROM` with the leading newline and indent every statement here uses.
/// Named because the joined reads below need it against a DIFFERENT table,
/// and two spellings of one clause is how their formatting drifts apart.
const FROM_CLAUSE = "\n  FROM ";

/// `UPDATE `, shared by the catalogue row write and the generation bump. Two
/// tables, one verb — named for the same reason as `FROM_CLAUSE`.
const UPDATE_CLAUSE = "UPDATE ";
const FROM_TABLE = FROM_CLAUSE ++ TABLE;

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
    UPDATE_CLAUSE ++ TABLE ++
    \\
    \\   SET context_cap_tokens = $2, input_nanos_per_mtok = $3,
    \\       cached_input_nanos_per_mtok = $4, output_nanos_per_mtok = $5,
    \\       updated_at_ms = $6
    \\ WHERE uid = $1::uuid
    ;

/// Delete the row identified by uid. Affected 0 → no such uid (caller → 404).
pub const DELETE_BY_UID =
    "DELETE FROM " ++ TABLE ++ " WHERE uid = $1::uuid";

/// One model's rate PLUS the catalogue generation it is read at, in ONE
/// statement (model_rate_cache.zig).
///
/// One statement is one snapshot, so the generation and the row are consistent
/// by construction — there is no window for the counter to advance between
/// reading it and reading the rate. Two statements would need an explicit
/// transaction to claim as much, and a caller that forgot one would cache a rate
/// under a generation it does not belong to. That is the whole failure this
/// column exists to prevent, so it is not left to a caller to get right.
///
/// The join is driven FROM the singleton, not from the catalogue: a LEFT JOIN
/// this way round still yields the revision on one row when the model is absent,
/// where an inner join would return nothing and leave the caller unable to tell
/// "no such model" from "could not read the generation". Those two answers get
/// different treatment — one is null, the other fails closed.
pub const LOAD_RATE_WITH_REVISION =
    "SELECT r.revision, " ++ RATE_COLUMNS_JOINED ++
    FROM_CLAUSE ++ REVISION_TABLE ++ " r" ++
    "\n  LEFT JOIN " ++ TABLE ++ " m" ++
    "\n    ON m.provider = $1 AND m.model_id = $2" ++
    "\n WHERE r.id = 1";

/// Rates for a SET of `(provider, model_id)` pairs — ONE statement, whatever the
/// page size.
///
/// The tenant registry page renders a rate beside every row plus one for the
/// platform default. Resolving those one at a time would make its statement
/// count a function of `limit`, which is the unbounded shape §3's budget exists
/// to forbid; a resident-only cache read costs nothing but answers null for
/// every row until some unrelated billing charge happens to load that exact
/// pair, so the page renders blank rates indefinitely after a restart.
///
/// Two parallel text arrays rather than a generated `IN (($1,$2),($3,$4),…)`
/// list: the statement text is then constant, so it plans once and is not a new
/// prepared statement per distinct page size.
///
/// `(provider, model_id)` is unique in this table, so each pair matches at most
/// one row. The identity is projected back so the caller can fill its positional
/// slots — the same shape `vault.loadMetadata` uses, and for the same reason.
pub const LOAD_RATES_FOR_PAIRS =
    "SELECT provider, model_id, " ++ RATE_COLUMNS ++
    FROM_TABLE ++
    "\n WHERE (provider, model_id) IN (SELECT p, m FROM unnest($1::text[], $2::text[]) AS u(p, m))";

/// Smallest context window any provider publishes for `model_id`.
///
/// A catalogue-wide aggregate, deliberately answered by the database rather than
/// by scanning the rate cache. The cache is bounded, so a scan of it answers
/// "the minimum among the rows that happen to be resident" — which is not the
/// question, and is a larger number than the truth whenever the true minimum was
/// the evicted row. A context budget above the real window fails the request
/// mid-run at the provider.
pub const MIN_CONTEXT_CAP_FOR_MODEL =
    "SELECT MIN(context_cap_tokens)::int" ++ FROM_TABLE ++ "\n WHERE model_id = $1";

// ── The catalogue generation (core.model_catalogue_revision) ────────────────
//
// The table is CHECK-constrained to a single row, but every statement still
// addresses it explicitly: a statement that leaned on the constraint would pass
// today and become a full-table write the moment anyone widened the table.

const WHERE_SINGLETON = " WHERE id = 1";

/// Hot-path read. Deliberately NO lock — a reader needs *a* consistent
/// generation, not the newest one, and locking here would serialize every
/// catalogue read behind the occasional admin write for no correctness gain.
pub const SELECT_REVISION = "SELECT revision FROM " ++ REVISION_TABLE ++ WHERE_SINGLETON;

/// Mutation path. `FOR UPDATE` is the serialization point between concurrent
/// admin writers: two of them must not both read N and both write N+1, which
/// would leave two different catalogue states sharing one generation.
pub const LOCK_REVISION = SELECT_REVISION ++ " FOR UPDATE";

/// `revision = revision + 1` is computed by the database under the row lock. A
/// caller-supplied next value would be a read-modify-write across the
/// application boundary — exactly the lost update the lock prevents.
pub const BUMP_REVISION =
    UPDATE_CLAUSE ++ REVISION_TABLE ++
    "\n   SET revision = revision + 1, updated_at_ms = $1" ++
    "\n" ++ WHERE_SINGLETON ++
    "\nRETURNING revision";
