//! Every statement the tenant model registry runs, and nothing else.
//!
//! The registry rows are `model.tenant_models`, and the display rate is read
//! from `model.platform_models`. Each is a `.sql` file beside this module,
//! checked against the rebuilt schema by `afd_db`'s
//! `test_every_sql_file_prepares`, and collected here for the reason
//! [`super::super::sql`] gives: REVIEW reads each statement side by side with
//! the schema it touches.
//!
//! Separate from that module because the split is a real seam: those
//! statements resolve and write the ONE selection row a tenant runs on, these
//! read and write the MANY registry rows it may choose from.
//!
//! # One projection, five spellings
//!
//! Four columns — `id`, `model_name`, `secret_ref`, `created_at` — and every
//! read and both `RETURNING` clauses name them identically, because the
//! registry's row type decodes them BY NAME. `tenant_id` is the predicate
//! rather than a column anyone reads back, and `updated_at` reaches no caller,
//! so neither is projected.
//!
//! # The page order
//!
//! `created_at DESC, id DESC` matches `idx_tenant_models_tenant_id_created_at`
//! and ends in the row id, which is not decoration: two entries sharing a
//! creation millisecond need a tiebreak, or the seek skips one of them.
//!
//! The collation is native on purpose — `id` is a `UUID` column, and casting it
//! to text to sort would make the index unusable. Canonical lowercase `UUIDv7`
//! sorts identically by byte and by ASCII, so the native ordering is the same
//! sequence with the index intact.

/// The registry's first page.
///
/// `$1` tenant · `$2` how many rows to fetch.
///
/// `$2` is `limit + 1`: the extra row is never returned, it only answers "is
/// there another page?" without a second `COUNT`.
pub const SELECT_FIRST_PAGE: &str = include_str!("select_first_page.sql");

/// The registry's subsequent pages.
///
/// `$1` tenant · `$2` boundary instant · `$3` boundary id · `$4` how many rows.
///
/// Two statements rather than one with a nullable boundary: an `OR $2 IS NULL`
/// would read as one statement and plan as a filter. The row comparison stops
/// being a range start on the composite index the moment it sits under an
/// `OR`, so the first page would keep its plan and every later page would scan.
///
/// The row-wise `(created_at, id) < ($2, $3)` is deliberately not the expanded
/// `created_at < $2 OR (created_at = $2 AND id < $3)`. The two are equivalent,
/// only the row form is matched to the index as a single range start, and the
/// expanded form is where off-by-one bugs live when someone edits one arm.
///
/// A boundary row deleted between pages is not a problem: the comparison is
/// against VALUES, not against a row that must still exist. That is what lets a
/// stale cursor continue rather than fail.
pub const SELECT_PAGE_AFTER: &str = include_str!("select_page_after.sql");

/// One entry by id, scoped to its tenant.
///
/// `$1` id · `$2` tenant.
///
/// The delete path needs the row's `secret_ref` BEFORE it can open the
/// reference transaction — the shared lock order is keyed on the credential, so
/// the credential has to be named first.
pub const SELECT_ENTRY: &str = include_str!("select_entry.sql");

/// Adds an entry, or reports that the tenant already has this exact one.
///
/// `$1` id · `$2` tenant · `$3` model · `$4` secret ref. Both stamps are the
/// database's `now()`.
///
/// `ON CONFLICT … DO NOTHING` rather than letting the unique violation raise:
/// zero affected rows IS the duplicate answer, and it costs one round trip with
/// no aborted subtransaction. `RETURNING` then makes the created row the
/// response's own — an empty result is the conflict, and there is no second
/// read that could observe a racing writer's row instead.
///
/// Safe from the multi-index `ON CONFLICT` deadlock class for the reason
/// [`super::super::sql::INSERT_TENANT_MODEL_IF_ABSENT`] gives: `id` is freshly
/// minted on every call, so only the domain key can ever be the arbiter.
pub const INSERT_ENTRY: &str = include_str!("insert_entry.sql");

/// Points an entry at a different model. The credential is immutable here.
///
/// `$1` id · `$2` tenant · `$3` model. `updated_at` is the database's `now()`.
///
/// An UPDATE, deliberately not an upsert: zero affected rows means the id does
/// not resolve for this tenant, which the caller reports as `UZ-MODELS-004`. An
/// upsert would CREATE a row under an id the caller invented, which is neither
/// verb's job.
pub const UPDATE_ENTRY_MODEL: &str = include_str!("update_entry_model.sql");

/// Removes an entry, inside the transaction that locked its credential.
///
/// `$1` id · `$2` tenant.
pub const DELETE_ENTRY: &str = include_str!("delete_entry.sql");

/// The catalogue's display rates for a page's `(provider, model)` pairs.
///
/// `$1` the providers · `$2` the models, positionally paired.
///
/// One statement whatever the page size, and that independence is the property
/// the read budget pins — not the number itself. A per-row lookup would make
/// the statement count a function of `limit`.
///
/// This is a DISPLAY read and deliberately fills no cache. Admitting rows from
/// here into the billing rate cache would let a later charge accept an entry
/// whose catalogue generation nothing checked, and would be a second way to
/// fill one cache.
pub const SELECT_RATES_FOR_PAIRS: &str = include_str!("select_rates_for_pairs.sql");
