//! Priced-model catalogue HTTP adapters.

use std::borrow::Cow;
use std::sync::Arc;

use afd_admin::{CreateModel, DeleteModel, Model, ModelInput};
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_wire::admin::{
    AdminModelCreate, AdminModelCreated, AdminModelItem, AdminModelUpdated, AdminModelsResponse,
    ModelRates,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use crate::auth::PersonIdentity;
use crate::handler::{refuse, reject};
use crate::request_id::RequestId;
use crate::services::Services;

const DETAIL_BODY_REQUIRED: &str = "Request body required";
const DETAIL_MALFORMED_JSON: &str = "Malformed JSON";
const DETAIL_PROVIDER_LEN: &str = "provider must be 1–64 chars";
const DETAIL_MODEL_ID_LEN: &str = "model_id must be 1–256 chars";
const DETAIL_CAP_POSITIVE: &str = "context_cap_tokens must be > 0";
const DETAIL_RATES_NONNEGATIVE: &str = "rates (input/cached/output nanos_per_mtok) must be >= 0";
const DETAIL_ID: &str = "id must be a canonical UUIDv7";
const DETAIL_NOT_FOUND: &str = "No catalogue model matches this id";
const DETAIL_DUPLICATE: &str = "A catalogue row for this provider and model already exists";
const DETAIL_IN_USE: &str =
    "This model is the active platform default; repoint the default before deleting it";

/// Lists every priced catalogue row.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/admin/models",
    tag = afd_http::openapi::tag::ADMIN,
    operation_id = "list_admin_models",
    summary = "List models",
    description = concat!(
        "Returns every shared model, ordered by provider and model ",
        "identifier. Requires the `model:read` scope. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = AdminModelsResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn list<D: Services>(State(services): State<Arc<D>>) -> Response {
    match services.models().list().await {
        Ok(models) => Json(AdminModelsResponse {
            models: models.iter().map(item).collect(),
            request_id: request_id(),
        })
        .into_response(),
        Err(error) => refuse(&error, "admin_models_list_failed"),
    }
}

/// Creates one immutable provider/model identity with mutable rates.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/admin/models",
    tag = afd_http::openapi::tag::ADMIN,
    operation_id = "create_admin_model",
    summary = "Add a priced model to the catalogue",
    description = concat!(
        "Creates one catalogue row. `(provider, model_id)` is the row's ",
        "immutable identity — change either by deleting and re-adding. The ",
        "rate cache is repopulated on success, so a new price is live with no ",
        "restart. Requires the `model:admin` scope. ",
    ),
    responses(
        (status = 201, description = afd_http::openapi::CREATED),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 500, description = afd_http::openapi::INTERNAL),
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
    let input = ModelInput::new(
        request.provider.into_owned(),
        request.model_id.into_owned(),
        store_rates(request.rates),
    );
    match services.models().create(&input, services.now()).await {
        Ok(CreateModel::Created(model)) => {
            let actor_id = identity.subject();
            let model_id = model.id().as_str();
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

/// Replaces one row's mutable context cap and rates.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/admin/models/{id}",
    tag = afd_http::openapi::tag::ADMIN,
    operation_id = "update_admin_model",
    summary = "Update a catalogue row's caps and rates",
    description = concat!(
        "Rates-only change; `provider` and `model_id` are the row identity ",
        "and are immutable on this endpoint. The rate cache is repopulated on ",
        "success. Requires the `model:admin` scope. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn update<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(raw_id): Path<String>,
    body: Bytes,
) -> Response {
    let id = match Uuid7::parse(&raw_id) {
        Ok(id) => id,
        Err(_error) => return reject(error_code::INVALID_REQUEST, DETAIL_ID),
    };
    let rates = match rates_request(&body) {
        Ok(rates) => rates,
        Err(detail) => return reject(error_code::INVALID_REQUEST, detail),
    };
    match services.models().update(&id, rates, services.now()).await {
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
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn delete<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(raw_id): Path<String>,
) -> Response {
    let id = match Uuid7::parse(&raw_id) {
        Ok(id) => id,
        Err(_error) => return reject(error_code::INVALID_REQUEST, DETAIL_ID),
    };
    match services.models().delete(&id, services.now()).await {
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

fn create_request(body: &[u8]) -> Result<AdminModelCreate<'_>, &'static str> {
    if body.is_empty() {
        return Err(DETAIL_BODY_REQUIRED);
    }
    let request = afd_core::json::object_from_slice::<AdminModelCreate<'_>>(body)
        .map_err(|_error| DETAIL_MALFORMED_JSON)?;
    if request.provider.is_empty() || request.provider.len() > 64 {
        return Err(DETAIL_PROVIDER_LEN);
    }
    if request.model_id.is_empty() || request.model_id.len() > 256 {
        return Err(DETAIL_MODEL_ID_LEN);
    }
    validate_rates(request.rates)?;
    Ok(request)
}

fn rates_request(body: &[u8]) -> Result<afd_admin::ModelRates, &'static str> {
    if body.is_empty() {
        return Err(DETAIL_BODY_REQUIRED);
    }
    let rates = afd_core::json::object_from_slice::<ModelRates>(body)
        .map_err(|_error| DETAIL_MALFORMED_JSON)?;
    validate_rates(rates)?;
    Ok(store_rates(rates))
}

fn validate_rates(rates: ModelRates) -> Result<(), &'static str> {
    if rates.context_cap_tokens <= 0 {
        return Err(DETAIL_CAP_POSITIVE);
    }
    if rates.input_nanos_per_mtok < 0
        || rates.cached_input_nanos_per_mtok < 0
        || rates.output_nanos_per_mtok < 0
    {
        return Err(DETAIL_RATES_NONNEGATIVE);
    }
    Ok(())
}

fn store_rates(rates: ModelRates) -> afd_admin::ModelRates {
    afd_admin::ModelRates::new(
        rates.context_cap_tokens,
        rates.input_nanos_per_mtok,
        rates.cached_input_nanos_per_mtok,
        rates.output_nanos_per_mtok,
    )
}

fn item(model: &Model) -> AdminModelItem<'static> {
    let rates = model.rates();
    AdminModelItem {
        id: Cow::Owned(model.id().to_string()),
        provider: Cow::Owned(model.provider().to_owned()),
        model_id: Cow::Owned(model.model_id().to_owned()),
        rates: ModelRates {
            context_cap_tokens: rates.context_cap_tokens(),
            input_nanos_per_mtok: rates.input_nanos_per_mtok(),
            cached_input_nanos_per_mtok: rates.cached_input_nanos_per_mtok(),
            output_nanos_per_mtok: rates.output_nanos_per_mtok(),
        },
    }
}

fn request_id() -> Cow<'static, str> {
    Cow::Owned(RequestId::mint().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &[u8] = br#"{"provider":"anthropic","model_id":"claude-opus-5","context_cap_tokens":200000,"input_nanos_per_mtok":5,"cached_input_nanos_per_mtok":1,"output_nanos_per_mtok":25}"#;

    #[test]
    fn create_validation_pins_every_bound() {
        assert_eq!(create_request(VALID).map(|_request| ()), Ok(()));
        assert_eq!(create_request(b""), Err(DETAIL_BODY_REQUIRED));
        assert_eq!(create_request(b"[]"), Err(DETAIL_MALFORMED_JSON));
        assert_eq!(
            create_request(br#"{"provider":"","model_id":"x","context_cap_tokens":1,"input_nanos_per_mtok":0,"cached_input_nanos_per_mtok":0,"output_nanos_per_mtok":0}"#),
            Err(DETAIL_PROVIDER_LEN)
        );
        assert_eq!(
            create_request(br#"{"provider":"x","model_id":"x","context_cap_tokens":0,"input_nanos_per_mtok":0,"cached_input_nanos_per_mtok":0,"output_nanos_per_mtok":0}"#),
            Err(DETAIL_CAP_POSITIVE)
        );
        assert_eq!(
            create_request(br#"{"provider":"x","model_id":"x","context_cap_tokens":1,"input_nanos_per_mtok":-1,"cached_input_nanos_per_mtok":0,"output_nanos_per_mtok":0}"#),
            Err(DETAIL_RATES_NONNEGATIVE)
        );
    }
}
