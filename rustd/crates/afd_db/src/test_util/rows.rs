//! Cleanup is armed before seeding, so failed assertions release their own rows.

use std::time::Duration;

use afd_core::error_code::{Coded as _, Logged};
use sqlx::Connection as _;

use super::TestDatabase;
use crate::{Result, error};

const CONTEXT: &str = "remove fixture-owned rows";
const ROLE: &str = "test fixture cleanup";
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct Row {
    statement: &'static str,
    identifier: String,
}

/// Deletes only the row identities a fixture registers, including on unwind.
///
/// Register parent rows before seeding. Statements execute in registration
/// order, so receipts without foreign keys can be removed before their owner.
#[derive(Debug)]
#[must_use = "keep the cleanup guard alive until the fixture is finished"]
pub struct OwnedRows {
    url: String,
    rows: Vec<Row>,
}

impl OwnedRows {
    /// Arms a guard against the lane this fixture actually uses.
    pub fn new(lane: &TestDatabase) -> Self {
        Self {
            url: lane.url(),
            rows: Vec::new(),
        }
    }

    /// Registers a static, single-parameter cleanup statement and its owned ID.
    pub fn record(&mut self, statement: &'static str, identifier: &str) {
        self.rows.push(Row {
            statement,
            identifier: identifier.to_owned(),
        });
    }

    /// Removes the registered rows and disarms unwind cleanup after success.
    ///
    /// # Errors
    /// Retains connection and statement causes; a failed cleanup stays armed.
    pub async fn finish(&mut self) -> Result<()> {
        remove(&self.url, &self.rows).await?;
        self.rows.clear();
        Ok(())
    }
}

impl Drop for OwnedRows {
    fn drop(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let url = self.url.clone();
        let rows = std::mem::take(&mut self.rows);
        // The dropping test may use a current-thread runtime. A separate
        // thread avoids waiting for I/O on the runtime whose unwind we hold.
        let removed = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap_or_else(|failure| panic!("fixture cleanup runtime must start: {failure}"));
            runtime.block_on(async {
                tokio::time::timeout(TIMEOUT, remove(&url, &rows))
                    .await
                    .unwrap_or_else(|_elapsed| Err(error::unreachable_datastore(ROLE, TIMEOUT)))
            })
        })
        .join();
        let Some(Logged { error_code, reason }) = unremoved(removed) else {
            return;
        };
        tracing::warn!(error_code, reason, event = NOT_REMOVED);
    }
}

/// The line a cleanup that left its rows behind is logged under.
const NOT_REMOVED: &str = "test_owned_rows_not_removed";

/// What a cleanup thread's panic says in place of a failure's sentence.
const CLEANUP_PANICKED: &str = "fixture cleanup thread panicked";

/// Why a cleanup left its rows behind, as its log line carries it, or `None`
/// when it removed them.
fn unremoved(removed: std::thread::Result<Result<()>>) -> Option<Logged> {
    match removed {
        Ok(Ok(())) => None,
        Ok(Err(cause)) => Some(cause.logged()),
        Err(_panicked) => Some(Logged {
            error_code: afd_core::error_code::INTERNAL_DB_UNAVAILABLE.as_str(),
            reason: CLEANUP_PANICKED.to_owned(),
        }),
    }
}

async fn remove(url: &str, rows: &[Row]) -> Result<()> {
    let mut connection = sqlx::PgConnection::connect(url).await.map_err(|source| {
        crate::Error::new(error::ErrorKind::DatastoreUnavailable { role: ROLE, source })
    })?;
    let mut tx = connection
        .begin()
        .await
        .map_err(|source| error::query(CONTEXT, source))?;
    for row in rows {
        sqlx::query(row.statement)
            .bind(&row.identifier)
            .execute(&mut *tx)
            .await
            .map_err(|source| error::query(CONTEXT, source))?;
    }
    tx.commit()
        .await
        .map_err(|source| error::query(CONTEXT, source))?;
    Ok(())
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the library"
)]
mod tests {
    use afd_core::error_code::{self, Coded as _};
    use afd_core::test_util::trace::Capture;

    use super::{CLEANUP_PANICKED, NOT_REMOVED, OwnedRows, Row, unremoved};

    /// A Postgres nobody listens on: port 1 is reserved and unbound.
    const NOWHERE: &str = "postgres://fixture:fixture@127.0.0.1:1/fixture";

    /// A guard armed against `NOWHERE`, holding one row.
    fn stranded() -> OwnedRows {
        OwnedRows {
            url: NOWHERE.to_owned(),
            rows: vec![Row {
                statement: "DELETE FROM fleet.fleets WHERE id = $1::uuid",
                identifier: "0199f0c0-0000-7000-8000-000000000001".to_owned(),
            }],
        }
    }

    /// A cleanup that cannot reach its database fails as an outage and stays
    /// armed, so the drop still owes the rows a removal.
    #[tokio::test]
    async fn a_cleanup_with_no_database_fails_as_an_outage_and_stays_armed() {
        let mut owned = stranded();

        let failure = owned.finish().await.expect_err("nothing listens on port 1");

        assert!(failure.is_datastore_unavailable(), "{failure}");
        assert_eq!(failure.code(), error_code::INTERNAL_DB_UNAVAILABLE);
        assert_eq!(owned.rows.len(), 1, "a failed cleanup keeps its rows armed");
    }

    /// The drop that cannot remove its rows says so once, with the failure's
    /// code beside an uncoded sentence, rather than panicking mid-unwind.
    #[test]
    fn a_drop_that_cannot_remove_its_rows_logs_the_outage() {
        let capture = Capture::install();

        drop(stranded());

        let line = capture.only(NOT_REMOVED);
        assert_eq!(
            line.field("error_code"),
            Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
        );
        line.assert_uncoded_reason();
    }

    /// A removal that ran says nothing; a failed one keeps its code beside its
    /// sentence; a panicked thread, which has no failure to name, still reads
    /// as the datastore cleanup it was.
    #[test]
    fn each_cleanup_ending_reads_as_its_own_line() {
        assert_eq!(unremoved(Ok(Ok(()))), None);
        for (kind, failure) in crate::error::one_of_each_kind() {
            let logged = failure.logged();
            assert_eq!(
                unremoved(Ok(Err(failure))),
                Some(logged),
                "{kind}: the line keeps the failure's code beside its sentence"
            );
        }
        let panicked = unremoved(Err(Box::new("boom"))).expect("a panic leaves rows behind");
        assert_eq!(
            panicked.error_code,
            error_code::INTERNAL_DB_UNAVAILABLE.as_str()
        );
        assert_eq!(panicked.reason, CLEANUP_PANICKED);
    }
}
