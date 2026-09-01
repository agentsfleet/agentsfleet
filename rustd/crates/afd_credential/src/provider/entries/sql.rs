//! Every statement the tenant model registry runs, and nothing else.
//!
//! Ports of `state/tenant_model_entries/sql.zig` and the display rate read from
//! `state/model_library/sql.zig`. Collected here for the reason
//! [`super::super::sql`] gives: REVIEW reading these side by side against the
//! Zig is the only enforcement of row-equivalence, and a statement inside a
//! function body cannot be read that way.
//!
//! Separate from that module rather than appended to it because both would then
//! be over the file cap, and the split is a real seam: those statements resolve
//! and write the ONE selection row a tenant runs on, these read and write the
//! MANY registry rows it may choose from.
//!
//! # The projection is one literal, shared
//!
//! Four columns — `id`, `model_id`, `secret_ref`, `created_at` — and every read
//! and both `RETURNING` clauses use the same spelling. `tenant_id` is the
//! predicate rather than a column anyone reads back, and `updated_at` reaches no
//! caller, so neither is projected: a narrower row than the Zig's, carrying
//! exactly what the wire and the cursor need.

/// The columns every read and both writes project.
///
/// A macro expanding to a LITERAL rather than a `const`, because `concat!` takes
/// literals only, and six hand-kept copies of one column list would drift the
/// moment the page gained a field (RULE UFS). The same shape
/// [`crate::vault::sql`]'s write arms share.
macro_rules! projection {
    () => {
        "id::text, model_id, secret_ref, created_at"
    };
}

/// The projection and the table, which every read opens with.
///
/// Named so the column list and the table cannot drift between the two page
/// statements and the by-id read — the same seam
/// `state/tenant_model_entries/sql.zig` draws with its own `SELECT_PROJECTION`.
macro_rules! select_projection {
    () => {
        concat!(
            "SELECT ",
            projection!(),
            "\n  FROM core.tenant_model_entries"
        )
    };
}

/// The tenant-scoped read every page statement opens with.
macro_rules! select_for_tenant {
    () => {
        concat!(select_projection!(), "\n WHERE tenant_id = $1::uuid")
    };
}

/// The page order, shared so the two page statements cannot drift.
///
/// `created_at DESC, id DESC` matches `idx_tenant_model_entries_tenant_id_created_at`
/// and ends in the row id, which is not decoration: two entries sharing a
/// creation millisecond need a tiebreak, or the seek below skips one of them.
///
/// The Zig's note on the collation applies unchanged — the spec writes the key
/// as `id COLLATE "C" DESC`, `id` is a `UUID` column, and casting it to text to
/// honour that literally would make the index unusable for the sort. Canonical
/// lowercase `UUIDv7` sorts identically by byte and by ASCII, so the native
/// ordering is the same sequence with the index intact.
macro_rules! order_by_keyset {
    () => {
        "\n ORDER BY created_at DESC, id DESC"
    };
}

/// The registry's first page.
///
/// `$1` tenant · `$2` how many rows to fetch.
///
/// `$2` is `limit + 1`: the extra row is never returned, it only answers "is
/// there another page?" without a second `COUNT`.
pub const SELECT_FIRST_PAGE: &str =
    concat!(select_for_tenant!(), order_by_keyset!(), "\n LIMIT $2");

/// The registry's subsequent pages.
///
/// `$1` tenant · `$2` boundary instant · `$3` boundary id · `$4` how many rows.
///
/// Two statements rather than one with a nullable boundary, and the duplication
/// is bounded to the `WHERE` clause because everything else is shared above.
/// An `OR $2 IS NULL` would read as one statement and plan as a filter: the row
/// comparison stops being a range start on the composite index the moment it
/// sits under an `OR`, so the first page would keep its plan and every later
/// page would scan.
///
/// The row-wise `(created_at, id) < ($2, $3)` is deliberately not the expanded
/// `created_at < $2 OR (created_at = $2 AND id < $3)`. The two are equivalent,
/// only the row form is matched to the index as a single range start, and the
/// expanded form is where off-by-one bugs live when someone edits one arm.
///
/// A boundary row deleted between pages is not a problem: the comparison is
/// against VALUES, not against a row that must still exist. That is what lets a
/// stale cursor continue rather than fail.
pub const SELECT_PAGE_AFTER: &str = concat!(
    select_for_tenant!(),
    "\n   AND (created_at, id) < ($2::bigint, $3::uuid)",
    order_by_keyset!(),
    "\n LIMIT $4"
);

/// One entry by id, scoped to its tenant.
///
/// `$1` id · `$2` tenant.
///
/// The delete path needs the row's `secret_ref` BEFORE it can open the
/// reference transaction — the shared lock order is keyed on the credential, so
/// the credential has to be named first.
pub const SELECT_ENTRY: &str = concat!(
    select_projection!(),
    "\n WHERE id = $1::uuid AND tenant_id = $2::uuid"
);

/// Adds an entry, or reports that the tenant already has this exact one.
///
/// `$1` id · `$2` tenant · `$3` model · `$4` secret ref · `$5` now.
///
/// `ON CONFLICT … DO NOTHING` rather than letting the unique violation raise:
/// zero affected rows IS the duplicate answer, and it costs one round trip with
/// no aborted subtransaction. `RETURNING` then makes the created row the
/// response's own — an empty result is the conflict, and there is no second
/// read that could observe a racing writer's row instead.
///
/// The table carries two unique indexes, the `id` primary key and the domain
/// key, and `ON CONFLICT` across several is where Postgres's unprincipled
/// deadlocks live. This is safe from that class for the same reason
/// [`super::super::sql::INSERT_MODEL_ENTRY_IF_ABSENT`] is: `id` is a freshly
/// minted uuidv7 on every call, so the primary key can never be the arbiter
/// that conflicts, and the hazard needs two.
pub const INSERT_ENTRY: &str = concat!(
    "INSERT INTO core.tenant_model_entries\n\
     \x20   (id, tenant_id, model_id, secret_ref, created_at, updated_at)\n\
     VALUES ($1::uuid, $2::uuid, $3, $4, $5, $5)\n\
     ON CONFLICT (tenant_id, model_id, secret_ref) DO NOTHING\n\
     RETURNING ",
    projection!()
);

/// Points an entry at a different model. The credential is immutable here.
///
/// `$1` id · `$2` tenant · `$3` model · `$4` now.
///
/// An UPDATE, deliberately not an upsert: zero affected rows means the id does
/// not resolve for this tenant, which the caller reports as `UZ-MODELS-004`. An
/// upsert would CREATE a row under an id the caller invented, which is neither
/// verb's job.
pub const UPDATE_ENTRY_MODEL: &str = concat!(
    "UPDATE core.tenant_model_entries\n\
     \x20  SET model_id = $3, updated_at = $4\n\
     WHERE id = $1::uuid AND tenant_id = $2::uuid\n\
     RETURNING ",
    projection!()
);

/// Removes an entry, inside the transaction that locked its credential.
///
/// `$1` id · `$2` tenant.
pub const DELETE_ENTRY: &str = "\
DELETE FROM core.tenant_model_entries
 WHERE id = $1::uuid AND tenant_id = $2::uuid";

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
/// fill one cache — see `state/model_rate_batch.zig`, which exists as its own
/// module so there is no cache in scope to populate by accident.
pub const SELECT_RATES_FOR_PAIRS: &str = "\
SELECT provider, model_id, context_cap_tokens,
       input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok
  FROM core.model_library
 WHERE (provider, model_id) IN (SELECT p, m FROM unnest($1::text[], $2::text[]) AS u(p, m))";

#[cfg(test)]
mod tests {
    use super::{
        DELETE_ENTRY, INSERT_ENTRY, SELECT_ENTRY, SELECT_FIRST_PAGE, SELECT_PAGE_AFTER,
        SELECT_RATES_FOR_PAIRS, UPDATE_ENTRY_MODEL,
    };

    /// The columns every read and both writes are supposed to answer with.
    const PROJECTED: [&str; 4] = ["id::text", "model_id", "secret_ref", "created_at"];

    #[test]
    fn every_statement_answering_a_row_answers_the_same_columns() {
        // The drift this shared literal exists to prevent: a `RETURNING` that
        // had lost a column would still compile, and the response would carry a
        // field read off the wrong index.
        for statement in [
            SELECT_FIRST_PAGE,
            SELECT_PAGE_AFTER,
            SELECT_ENTRY,
            INSERT_ENTRY,
            UPDATE_ENTRY_MODEL,
        ] {
            for column in PROJECTED {
                assert!(
                    statement.contains(column),
                    "{statement}\ndoes not project {column}"
                );
            }
        }
    }

    #[test]
    fn every_statement_is_scoped_to_one_tenant() {
        // Tenancy is enforced in SQL and not by trusting the handler: an id
        // belonging to somebody else resolves no row rather than the wrong row.
        for statement in [
            SELECT_FIRST_PAGE,
            SELECT_PAGE_AFTER,
            SELECT_ENTRY,
            INSERT_ENTRY,
            UPDATE_ENTRY_MODEL,
            DELETE_ENTRY,
        ] {
            assert!(
                statement.contains("tenant_id"),
                "unscoped statement: {statement}"
            );
        }
    }

    #[test]
    fn both_page_statements_walk_the_same_order() {
        // Two pages of one walk ordered differently would silently drop rows
        // and repeat others, and the client would see neither.
        let order = "ORDER BY created_at DESC, id DESC";
        assert!(SELECT_FIRST_PAGE.contains(order));
        assert!(SELECT_PAGE_AFTER.contains(order));
    }

    #[test]
    fn the_later_page_seeks_by_row_comparison_and_not_by_an_expanded_or() {
        // Only the row form is matched to the composite index as a single range
        // start; the expanded form plans as a filter and is where off-by-one
        // bugs live.
        assert!(SELECT_PAGE_AFTER.contains("(created_at, id) < ($2::bigint, $3::uuid)"));
        assert!(!SELECT_PAGE_AFTER.contains(" OR "));
    }

    #[test]
    fn the_create_arm_reports_a_duplicate_and_never_overwrites_one() {
        assert!(INSERT_ENTRY.contains("ON CONFLICT (tenant_id, model_id, secret_ref) DO NOTHING"));
        assert!(
            !INSERT_ENTRY.contains("DO UPDATE"),
            "a create that finds the pair taken must not become a replace"
        );
    }

    #[test]
    fn the_model_change_is_an_update_and_creates_nothing() {
        // An upsert here would create a row under an id the caller invented,
        // rather than reporting that the id does not resolve.
        assert!(UPDATE_ENTRY_MODEL.starts_with("UPDATE core.tenant_model_entries"));
        assert!(!UPDATE_ENTRY_MODEL.contains("INSERT"));
    }

    #[test]
    fn the_model_change_cannot_move_an_entry_to_another_credential() {
        // `secret_ref` is immutable on this verb: a model row backed by a
        // different credential is a DIFFERENT entry, which is what the domain
        // key says. Setting it here would let one row silently become another.
        assert!(!UPDATE_ENTRY_MODEL.contains("secret_ref = "));
    }

    #[test]
    fn the_rate_read_asks_for_pairs_and_not_for_a_cross_product() {
        // `provider = ANY(...) AND model_id = ANY(...)` would match every
        // combination of the two lists and price a row against a provider that
        // does not serve it.
        assert!(SELECT_RATES_FOR_PAIRS.contains("(provider, model_id) IN"));
        assert!(SELECT_RATES_FOR_PAIRS.contains("unnest($1::text[], $2::text[])"));
    }
}
