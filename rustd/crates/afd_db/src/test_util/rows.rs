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
        let failure = match removed {
            Ok(Ok(())) => return,
            Ok(Err(cause)) => cause.logged(),
            Err(_panicked) => Logged {
                error_code: afd_core::error_code::INTERNAL_DB_UNAVAILABLE.as_str(),
                reason: "fixture cleanup thread panicked".to_owned(),
            },
        };
        let Logged { error_code, reason } = failure;
        tracing::warn!(error_code, reason, event = "test_owned_rows_not_removed");
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
