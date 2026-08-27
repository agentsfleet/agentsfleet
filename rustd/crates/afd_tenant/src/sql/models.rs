//! Statements for the model-library catalogue page — `core.model_library`.
//!
//! Ports of `state/model_library/sql.zig`'s §2 page statements. The
//! normalization is SQL-side there and stays SQL-side here, for the reason
//! that module records: folding BOTH the column and the compared value with
//! the same `lower(normalize(…, NFKC))` expression is what makes a match
//! independent of the script the caller typed in. `COLLATE "C"` rides every
//! sort key and every seek operand, so the ORDER BY and the predicate that
//! resumes it cannot compare under different rules.
//!
//! The tiebreak is `id`, not `(provider, model_id)`: normalization is
//! many-to-one, so two rows distinct in the table can share a folded key pair,
//! and a keyset whose last component can repeat either skips rows or loops on
//! them. `id` is ordered natively rather than as text — for canonical
//! lowercase UUIDs the two sequences are identical and the cast would force a
//! per-row conversion.
//!
//! The projection, filter and order are spelled in FULL in both statements,
//! the way `sql/billing.rs` spells its select list twice: `concat!` takes
//! only literals, and the Zig comptime `++` that assembled these has no
//! equally cheap Rust spelling. The duplication is the price of grep-able
//! statements, and this comment is the marker a drift-hunting reviewer greps
//! for.

/// First page. `$1` is the normalized provider filter or NULL; `$2` is
/// `limit + 1` — the extra row never reaches the response, it only answers
/// "is there another page?" without a COUNT.
pub const SELECT_LIBRARY_PAGE_FIRST: &str = "\
SELECT model_id, provider, context_cap_tokens, input_nanos_per_mtok, \
cached_input_nanos_per_mtok, output_nanos_per_mtok, updated_at, id::text, \
lower(normalize(model_id, NFKC)) AS display_key, \
lower(normalize(provider, NFKC)) AS vendor_key \
FROM core.model_library \
WHERE ($1::text IS NULL OR lower(normalize(provider, NFKC)) = lower(normalize($1, NFKC))) \
ORDER BY lower(normalize(model_id, NFKC)) COLLATE \"C\", \
lower(normalize(provider, NFKC)) COLLATE \"C\", id \
LIMIT $2";

/// Resume after a cursor.
///
/// The row-wise comparison is the seek predicate: strictly greater on the
/// first key that differs, which is exactly the order below. `$2` is the
/// `display_key`, `$3` the `vendor_key`, `$4` the id, `$5` `limit + 1`.
pub const SELECT_LIBRARY_PAGE_AFTER: &str = "\
SELECT model_id, provider, context_cap_tokens, input_nanos_per_mtok, \
cached_input_nanos_per_mtok, output_nanos_per_mtok, updated_at, id::text, \
lower(normalize(model_id, NFKC)) AS display_key, \
lower(normalize(provider, NFKC)) AS vendor_key \
FROM core.model_library \
WHERE ($1::text IS NULL OR lower(normalize(provider, NFKC)) = lower(normalize($1, NFKC))) \
AND (lower(normalize(model_id, NFKC)) COLLATE \"C\", \
lower(normalize(provider, NFKC)) COLLATE \"C\", id) \
> ($2::text COLLATE \"C\", $3::text COLLATE \"C\", $4::uuid) \
ORDER BY lower(normalize(model_id, NFKC)) COLLATE \"C\", \
lower(normalize(provider, NFKC)) COLLATE \"C\", id \
LIMIT $5";
