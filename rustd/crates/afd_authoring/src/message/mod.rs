//! Durable message requests and monotone cancellation; no transient turn resource.

mod accept;
mod cancel;
mod conversation;
mod read;

pub(crate) use accept::{Insertion, insert};
pub use cancel::{Cancellation, Stopped};
pub use conversation::Conversation;

pub(crate) const ACCEPTED: &str = "accepted";

pub(crate) use afd_fleet_runtime::definition::normalize_text as normalize;

pub(crate) async fn save_requested(
    connection: &mut sqlx::PgConnection,
    draft: &afd_core::id::Uuid7,
    sequence: i64,
) -> crate::Result<bool> {
    sqlx::query_scalar(include_str!("save_requested.sql"))
        .bind(draft.as_str())
        .bind(sequence)
        .fetch_one(connection)
        .await
        .map_err(crate::error::raise::query("read accepted save intent"))
}
