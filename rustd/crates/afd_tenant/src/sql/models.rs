//! Statements for the model-library catalogue page — `model.platform_models`.
//!
//! The normalization is SQL-side: folding BOTH the column and the compared
//! value with the same `lower(normalize(…, NFKC))` expression is what makes a
//! match independent of the script the caller typed in. `COLLATE "C"` rides every
//! sort key and every seek operand, so the ORDER BY and the predicate that
//! resumes it cannot compare under different rules.
//!
//! The tiebreak is `id`, not `(provider, model_name)`: normalization is
//! many-to-one, so two rows distinct in the table can share a folded key pair,
//! and a keyset whose last component can repeat either skips rows or loops on
//! them. `id` is ordered natively rather than as text — for canonical
//! lowercase UUIDs the two sequences are identical and the cast would force a
//! per-row conversion.
//!
//! The projection, filter and order are spelled in FULL in both files. The
//! duplication is the price of whole statements a `PREPARE` can check, and
//! this comment is the marker a drift-hunting reviewer greps for.

/// First page. `$1` is the normalized provider filter or NULL; `$2` is
/// `limit + 1` — the extra row never reaches the response, it only answers
/// "is there another page?" without a COUNT.
pub const SELECT_LIBRARY_PAGE_FIRST: &str = include_str!("models/select_library_page_first.sql");

/// Resume after a cursor.
///
/// The row-wise comparison is the seek predicate: strictly greater on the
/// first key that differs, which is exactly the order below. `$2` is the
/// `display_key`, `$3` the `vendor_key`, `$4` the id, `$5` `limit + 1`.
pub const SELECT_LIBRARY_PAGE_AFTER: &str = include_str!("models/select_library_page_after.sql");
