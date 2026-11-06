//! Priced-model catalogue HTTP adapters.

mod input;

use input::{create_request, patch_request};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use afd_admin::{CreateModel, DecisionModel, DeleteModel, Model, ModelInput, ModelUpdate};
use afd_api_wire::admin::{
    AdminDecisionModel, AdminModelCreated, AdminModelItem, AdminModelUpdated, AdminModelsResponse,
};
#[cfg(feature = "openapi")]
use afd_api_wire::admin::{AdminModelCreate, AdminModelPatch};
use afd_api_wire::catalogue::{CatalogueMetadata, CataloguePrice};
use afd_core::error_code;
use afd_http::services::{metadata_store, metadata_wire, price_store, price_wire};
use afd_wire::inference::ModelInference;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use crate::auth::PersonIdentity;
use crate::handler::{parse_id, refuse, reject};
use crate::request_id::RequestId;
use crate::services::Services;

const DETAIL_ID: &str = "id must be a canonical UUIDv7";
const DETAIL_NOT_FOUND: &str = "No catalogue model matches this id";
const DETAIL_DUPLICATE: &str = "A catalogue row for this provider and model already exists";
const DETAIL_IN_USE: &str =
    "This model is in use; change its platform default or runner assignments before deleting it";

/// Lists one snapshot-bound page of priced catalogue rows.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/admin/models",
    tag = afd_http::openapi::tag::ADMIN,
    operation_id = "list_admin_models",
    summary = "List models",
    description = concat!(
        "Returns a bounded page of shared models, ordered by provider and model ",
        "identifier. Continue with next_cursor as starting_after; catalogue edits invalidate cursors. Requires the `model:read` scope. ",
    ),
    params(
        ("limit" = Option<u32>, Query, description = "Page size, 1–100; defaults to 50"),
        ("starting_after" = Option<String>, Query, description = "Continuation cursor from this catalogue generation and page size"),
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = AdminModelsResponse),
        (status = 400, description = "Invalid page size or malformed, mismatched or stale cursor"),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let limit = match afd_validate::Limit::parse(
        params.get("limit").map(String::as_str),
        afd_core::paging::CEILING,
    ) {
        Ok(limit) => limit,
        Err(_invalid) => {
            return reject(
                error_code::INVALID_REQUEST,
                "limit must be an integer between 1 and 100",
            );
        }
    };
    match services
        .models()
        .page(limit, params.get("starting_after").map(String::as_str))
        .await
    {
        Ok(page) => Json(AdminModelsResponse {
            models: page.models.iter().map(item).collect(),
            next_cursor: page.next_cursor.map(Cow::Owned),
            request_id: request_id(),
        })
        .into_response(),
        Err(error) => refuse(&error, "admin_models_list_failed"),
    }
}

/// Creates one provider/model identity with mutable inference settings and rates.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/admin/models",
    tag = afd_http::openapi::tag::ADMIN,
    operation_id = "create_admin_model",
    summary = "Add a priced model to the catalogue",
    description = concat!(
        "Adds a model and its rates. Prices take effect without a restart. Requires ",
        "`model:admin`. To change `provider` or `model_id`, delete the entry and create ",
        "another.",
    ),
    request_body = AdminModelCreate,
    responses(
        (status = 201, description = afd_http::openapi::CREATED, body = AdminModelCreated),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn create<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    body: Bytes,
) -> Response {
    let request = match create_request(&body) {
        Ok(request) => request,
        Err(detail) => return reject(error_code::INVALID_REQUEST, detail),
    };
    let request = request.into_inner();
    let input = ModelInput {
        provider: request.provider.into_owned(),
        model_name: request.model_id.into_owned(),
        details: details(
            request.metadata,
            request.prices,
            request.inference,
            request.decision,
        ),
    };
    match services.models().create(input, services.now()).await {
        Ok(CreateModel::Created(model)) => {
            let actor_id = identity.subject();
            let model_id = model.id.as_str();
            tracing::info!(actor_id, model_id, event = "admin_model_created",);
            (
                StatusCode::CREATED,
                Json(AdminModelCreated {
                    model: item(&model),
                    request_id: request_id(),
                }),
            )
                .into_response()
        }
        Ok(CreateModel::Duplicate) => reject(error_code::PROVIDER_MODEL_EXISTS, DETAIL_DUPLICATE),
        Err(error) => refuse(&error, "admin_model_create_failed"),
    }
}

/// Updates mutable inference settings, context and rates.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/admin/models/{id}",
    tag = afd_http::openapi::tag::ADMIN,
    operation_id = "update_admin_model",
    summary = "Update model settings and rates",
    description = concat!(
        "Updates model rates and inference settings without a restart. Requires ",
        "`model:admin`. The `provider` and `model_id` fields cannot change.",
    ),
    request_body = AdminModelPatch,
    params(
        afd_http::openapi::path::Id,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = AdminModelUpdated),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn update<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(raw_id): Path<String>,
    body: Bytes,
) -> Response {
    let id = match parse_id(&raw_id, DETAIL_ID) {
        Ok(id) => id,
        Err(refused) => return refused.into_response(),
    };
    let request = match patch_request(&body) {
        Ok(request) => request.into_inner(),
        Err(detail) => return reject(error_code::INVALID_REQUEST, detail),
    };
    match services
        .models()
        .update(
            &id,
            details(
                request.metadata,
                request.prices,
                request.inference,
                request.decision,
            ),
            services.now(),
        )
        .await
    {
        Ok(true) => {
            let actor_id = identity.subject();
            let model_id = id.as_str();
            tracing::info!(actor_id, model_id, event = "admin_model_updated",);
            Json(AdminModelUpdated {
                id: Cow::Owned(id.to_string()),
                updated: true,
                request_id: request_id(),
            })
            .into_response()
        }
        Ok(false) => reject(error_code::PROVIDER_MODEL_NOT_FOUND, DETAIL_NOT_FOUND),
        Err(error) => refuse(&error, "admin_model_update_failed"),
    }
}

/// Deletes one row unless the active platform default references it.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/admin/models/{id}",
    tag = afd_http::openapi::tag::ADMIN,
    operation_id = "delete_admin_model",
    summary = "Remove a model",
    description = concat!(
        "Refuses to remove the active default model. Choose another default ",
        "first. Requires the `model:admin` scope. ",
    ),
    params(
        afd_http::openapi::path::Id,
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn delete<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(raw_id): Path<String>,
) -> Response {
    let id = match parse_id(&raw_id, DETAIL_ID) {
        Ok(id) => id,
        Err(refused) => return refused.into_response(),
    };
    match services.models().delete(&id).await {
        Ok(DeleteModel::Deleted) => {
            let actor_id = identity.subject();
            let model_id = id.as_str();
            tracing::info!(actor_id, model_id, event = "admin_model_deleted",);
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(DeleteModel::NotFound) => reject(error_code::PROVIDER_MODEL_NOT_FOUND, DETAIL_NOT_FOUND),
        Ok(DeleteModel::InUse) => reject(error_code::PROVIDER_MODEL_IN_USE, DETAIL_IN_USE),
        Err(error) => refuse(&error, "admin_model_delete_failed"),
    }
}

/// A body [`input`] proved, in the shape the store replaces a model with.
/// Only the two parsers' `Valid` bodies reach here.
fn details(
    metadata: CatalogueMetadata<'_>,
    prices: Vec<CataloguePrice<'_>>,
    inference: Option<ModelInference>,
    decision: Option<AdminDecisionModel<'_>>,
) -> ModelUpdate {
    ModelUpdate {
        metadata: metadata_store(metadata),
        prices: prices.into_iter().map(price_store).collect(),
        inference,
        decision: decision.map(|model| DecisionModel {
            configuration: afd_wire::decision::DecisionConfiguration {
                enabled: model.enabled,
                profile: model.profile.into_owned(),
            },
            prices: model.prices.into_iter().map(price_store).collect(),
        }),
    }
}

fn item(model: &Model) -> AdminModelItem<'_> {
    AdminModelItem {
        id: Cow::Owned(model.id.to_string()),
        provider: Cow::Owned(model.provider.clone()),
        model_id: Cow::Owned(model.model_name.clone()),
        metadata: metadata_wire(&model.details.metadata),
        prices: model.details.prices.iter().map(price_wire).collect(),
        inference: model.details.inference.clone(),
        decision: model
            .details
            .decision
            .as_ref()
            .map(|model| AdminDecisionModel {
                enabled: model.configuration.enabled,
                profile: Cow::Borrowed(&model.configuration.profile),
                prices: model.prices.iter().map(price_wire).collect(),
            }),
    }
}

fn request_id() -> Cow<'static, str> {
    Cow::Owned(RequestId::mint().to_string())
}
