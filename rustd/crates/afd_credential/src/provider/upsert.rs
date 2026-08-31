//! The write half of the tenant's selection row.
//!
//! [`selection`](super::selection) reads the two rows that say WHERE a key is;
//! this is the one statement that PUTS one there. It is a separate file for the
//! reason [`store`](super::store) gives — sibling modules add verbs to
//! [`Providers`] in their own files — and because the read module's whole
//! subject is what resolution may assume before anything is opened, which a
//! write does not share.
//!
//! # The coherent pair is the boundary's job, not this one's
//!
//! [`Posture`] carries no payload, so a [`Selection`] can still spell the two
//! states that mean nothing: platform mode naming a credential, and
//! self-managed mode naming none. Both are refused ONCE, where a request
//! becomes a `Selection` — the tenant handler answers the second with its own
//! registry code and a sentence a client can act on. This module writes the
//! pair it is handed rather than re-deciding it, so there is exactly one place
//! that says what a coherent selection is.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

use crate::error::{Result, provider_malformed, query};
use crate::provider::selection::Selection;
use crate::provider::sql;
use crate::provider::store::Providers;

/// Statement name, for the context a query failure carries.
const CONTEXT_UPSERT: &str = "tenant model selection write";

/// The field an out-of-range context ceiling is reported against.
const FIELD_CONTEXT_CAP: &str = "context_cap_tokens";

impl Providers {
    /// Writes `tenant_id`'s selection, last-write-wins on its single row.
    ///
    /// `now` is passed rather than read from a clock here, so the caller that
    /// already stamped the rest of its work uses one instant for all of it and
    /// a test can write a row at a time it chooses.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a context ceiling wider
    /// than the column holds.
    pub async fn upsert(
        &self,
        tenant_id: &Uuid7,
        selection: &Selection,
        now: UnixMillis,
    ) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query(sql::UPSERT_TENANT_MODEL_SELECTION)
            .bind(tenant_id.as_str())
            .bind(selection.posture.as_str())
            .bind(&*selection.provider)
            .bind(&*selection.model)
            .bind(stored_cap(selection.context_cap_tokens)?)
            .bind(selection.secret_ref.as_deref())
            .bind(now.as_millis())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_UPSERT))?;

        Ok(())
    }
}

/// A context ceiling, back in the column's signed width.
///
/// Reports rather than saturating, and the difference is what the tenant reads
/// back afterwards: a clamped write answers a ceiling nobody chose, and looks
/// like a working configuration. Nothing in this daemon can reach the refusal —
/// every cap on this path was read out of an `int4` column through
/// [`selection`](super::selection)'s own narrowing — which is exactly why it
/// costs nothing to keep.
fn stored_cap(requested: u32) -> Result<i32> {
    i32::try_from(requested).map_err(|_too_wide| provider_malformed(FIELD_CONTEXT_CAP))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::stored_cap;

    #[test]
    fn a_ceiling_the_column_holds_round_trips_unchanged() {
        for requested in [0, 1, 200_000, 1_048_576] {
            assert_eq!(
                stored_cap(requested).expect("a ceiling inside the column's width"),
                i32::try_from(requested).expect("the same width, from the other side")
            );
        }
        let widest = u32::try_from(i32::MAX).expect("i32::MAX is a positive count");
        assert_eq!(
            stored_cap(widest).expect("the widest the column holds is still held"),
            i32::MAX,
            "the ceiling itself is accepted — the refusal starts one token later"
        );
    }

    #[test]
    fn a_ceiling_wider_than_the_column_reports_rather_than_saturating() {
        // Saturating here would store `i32::MAX` and answer the next read with
        // a ceiling the tenant never chose. The whole point is that the write
        // fails loudly instead.
        let over = u32::try_from(i32::MAX).expect("i32::MAX is a positive count") + 1;
        for requested in [over, u32::MAX] {
            stored_cap(requested).expect_err("a ceiling the column cannot hold is not written");
        }
    }
}
