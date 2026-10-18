//! Statements stay with their domain owner; values and lifecycle words are bound.

pub(crate) mod access;
pub(crate) mod candidate;
pub(crate) mod draft;
pub(crate) mod message;

/// The database clock, read once after the caller's row locks were granted.
///
/// A validity check compares against this reading, never against `now()`: the
/// transaction's start can precede a lock wait that outlived the row's expiry.
/// Floored to the millisecond the `TIMESTAMPTZ(3)` columns hold.
pub(crate) const CLOCK: &str = include_str!("clock.sql");

/// Reads [`CLOCK`] on `connection`, attributing a failure to `context`.
pub(crate) async fn clock(
    connection: &mut sqlx::PgConnection,
    context: &'static str,
) -> crate::Result<jiff::Timestamp> {
    let now: jiff_sqlx::Timestamp = sqlx::query_scalar(CLOCK)
        .fetch_one(connection)
        .await
        .map_err(crate::error::raise::query(context))?;
    Ok(now.to_jiff())
}
