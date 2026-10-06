//! A fact a write proves on its own transaction before it changes anything.
//!
//! The rows a write changes and the rows that say whether the caller may still
//! write them can belong to different crates: a fleet's schedules are
//! `afd_cron`'s, and the lease that lets a runner change them is `afd_fleet`'s.
//! Checking the lease first and writing afterwards leaves a window in which a
//! reclaim lands between the two. Run on the write's own transaction, the check
//! takes its lock there and keeps it until the write commits.

use std::pin::Pin;

use sqlx::PgConnection;

/// What a guarded write proves before it writes.
///
/// `holds` runs on the write's transaction. Whatever it locks stays locked
/// until that transaction ends, so the fact it proved is still true when the
/// write commits.
pub trait Precondition: Send + Sync {
    /// Whether the write may go ahead.
    ///
    /// # Errors
    /// Reports a statement that failed.
    fn holds<'c>(
        &'c self,
        connection: &'c mut PgConnection,
    ) -> Pin<Box<dyn Future<Output = sqlx::Result<bool>> + Send + 'c>>;
}
