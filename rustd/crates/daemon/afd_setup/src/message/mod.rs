//! Durable message requests and monotone cancellation; no transient turn resource.

mod accept;
mod cancel;
mod conversation;
mod read;

pub(crate) use accept::{Insertion, insert};
pub use cancel::{Cancellation, Stopped};
pub use conversation::Conversation;

pub(crate) const ACCEPTED: &str = "accepted";

pub(crate) use af_fleet_runtime::definition::normalize_text as normalize;

#[derive(sqlx::FromRow)]
pub(crate) struct Accepted {
    pub(crate) save_requested: bool,
    pub(crate) user_text: String,
}

pub(crate) async fn accepted(
    connection: &mut sqlx::PgConnection,
    draft: &af_core::id::Uuid7,
    sequence: i64,
) -> crate::Result<Accepted> {
    sqlx::query_as(include_str!("accepted.sql"))
        .bind(draft)
        .bind(sequence)
        .fetch_one(connection)
        .await
        .map_err(crate::error::raise::query("read accepted save intent"))
}
