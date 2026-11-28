//! Check model capability while a newly staged image turn still owns the fleet lock.

use af_core::id::Uuid7;
use af_wire::turn::TurnKind;
use sqlx::PgConnection;
use std::pin::Pin;

use crate::{
    IncomingTurn, Producer, Result,
    error::{ErrorKind, query},
};

/// Resolves the capability of an image message's selected model in the
/// transaction that admits it. A name identifies a model that cannot take it.
pub trait ImageModelLookup: Send + Sync {
    /// Looks up the saved model for this owner, preserving resolution failures.
    fn refusing_model<'a>(
        &self,
        connection: &'a mut PgConnection,
        tenant: &'a Uuid7,
        reference: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + 'a>>;
}

pub(crate) fn has_images(turn: &IncomingTurn<'_>) -> bool {
    matches!(turn.producer, Producer::Steer)
        && matches!(turn.event_type, TurnKind::Chat)
        && af_wire::turn::has_images(turn.request_json)
}

pub(crate) async fn check(
    connection: &mut PgConnection,
    turn: &IncomingTurn<'_>,
    lookup: &dyn ImageModelLookup,
) -> Result<()> {
    // The allocating statement already proved scope and locked the row. A
    // model edit waits for this transaction, and a replay skips the check.
    let (tenant, reference): (Uuid7, String) = sqlx::query_as(include_str!("sql/image_model.sql"))
        .bind(turn.fleet)
        .bind(turn.workspace)
        .fetch_one(&mut *connection)
        .await
        .map_err(query("read the image message's fleet model"))?;
    if let Some(model) = lookup
        .refusing_model(connection, &tenant, &reference)
        .await?
    {
        return Err(ErrorKind::ImageModel { model }.into());
    }
    Ok(())
}
