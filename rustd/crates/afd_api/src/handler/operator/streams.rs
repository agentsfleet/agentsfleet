//! Live stream overview HTTP adapter.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use crate::services::Services;

pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
) -> Json<afd_wire::admin::FleetStreamsResponse<'static>> {
    Json(services.streams().overview())
}
