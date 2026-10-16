//! Catalogue statements and row decoding share one positional schema.

use super::{
    CONTEXT_LIST, CONTEXT_UPDATE, MODEL_TABLE, Model, ModelInference, ModelInput, ModelRates,
    Result, UnixMillis, Uuid7, query,
};
use crate::error::row;
use sqlx::Row as _;

pub(super) fn decode(row_value: &sqlx::postgres::PgRow) -> Result<Model> {
    let id: String = row_value.try_get(0).map_err(query(CONTEXT_LIST))?;
    Ok(Model {
        id: Uuid7::parse(&id).map_err(row(MODEL_TABLE, "id"))?,
        provider: row_value.try_get(1).map_err(query(CONTEXT_LIST))?,
        identifier: row_value.try_get(2).map_err(query(CONTEXT_LIST))?,
        inference: row_value
            .try_get::<Option<sqlx::types::Json<ModelInference>>, _>(7)
            .map_err(query(CONTEXT_LIST))?
            .map(|value| value.0),
        rates: ModelRates::new(
            row_value.try_get(3).map_err(query(CONTEXT_LIST))?,
            row_value.try_get(4).map_err(query(CONTEXT_LIST))?,
            row_value.try_get(5).map_err(query(CONTEXT_LIST))?,
            row_value.try_get(6).map_err(query(CONTEXT_LIST))?,
        ),
    })
}

pub(super) fn bind_create<'a>(
    query: afd_db::statement::Bound<'a>,
    id: &'a Uuid7,
    input: &'a ModelInput,
    now: UnixMillis,
) -> afd_db::statement::Bound<'a> {
    query
        .bind(id.as_str())
        .bind(&input.identifier)
        .bind(&input.provider)
        .bind(input.rates.context_cap_tokens())
        .bind(input.rates.input_nanos_per_mtok())
        .bind(input.rates.cached_input_nanos_per_mtok())
        .bind(input.rates.output_nanos_per_mtok())
        .bind(now.as_millis())
        .bind(input.inference.as_ref().map(sqlx::types::Json))
}

pub(super) fn bind_rates<'a>(
    query: afd_db::statement::Bound<'a>,
    id: &'a Uuid7,
    rates: ModelRates,
    inference: Option<&'a ModelInference>,
    now: UnixMillis,
) -> afd_db::statement::Bound<'a> {
    query
        .bind(id.as_str())
        .bind(rates.context_cap_tokens())
        .bind(rates.input_nanos_per_mtok())
        .bind(rates.cached_input_nanos_per_mtok())
        .bind(rates.output_nanos_per_mtok())
        .bind(now.as_millis())
        .bind(inference.map(sqlx::types::Json))
}

pub(super) async fn lock_revision(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    context: &'static str,
) -> Result<()> {
    sqlx::query("SELECT revision FROM core.model_catalogue_revision WHERE id = 1 FOR UPDATE")
        .execute(&mut **transaction)
        .await
        .map(|_| ())
        .map_err(query(context))
}

pub(super) async fn bump(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    now: UnixMillis,
    context: &'static str,
) -> Result<()> {
    sqlx::query("UPDATE core.model_catalogue_revision SET revision = revision + 1, updated_at = $1 WHERE id = 1")
        .bind(now.as_millis())
        .execute(&mut **transaction)
        .await
        .map(|_| ())
        .map_err(query(context))
}

pub(super) const LIST: &str = "SELECT id::text, provider, model_id, context_cap_tokens, input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok, inference FROM core.model_library ORDER BY provider, model_id";
pub(super) const CREATE: &str = "INSERT INTO core.model_library (id, model_id, provider, context_cap_tokens, input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at, inference) VALUES ($1::uuid,$2,$3,$4,$5,$6,$7,$8,$8,$9) ON CONFLICT (provider, model_id) DO NOTHING";
pub(super) const UPDATE: &str = "UPDATE core.model_library SET context_cap_tokens=$2,input_nanos_per_mtok=$3,cached_input_nanos_per_mtok=$4,output_nanos_per_mtok=$5,updated_at=$6,inference=COALESCE($7,inference) WHERE id=$1::uuid";
pub(super) const DELETE: &str = "WITH target AS (SELECT provider,model_id FROM core.model_library WHERE id=$1::uuid), blocked AS (SELECT 1 FROM target t JOIN core.platform_provider_defaults p ON p.provider=t.provider AND p.model=t.model_id AND p.active=true), removed AS (DELETE FROM core.model_library WHERE id=$1::uuid AND NOT EXISTS (SELECT 1 FROM blocked) RETURNING 1) SELECT CASE WHEN EXISTS(SELECT 1 FROM removed) THEN 'deleted' WHEN EXISTS(SELECT 1 FROM blocked) THEN 'in_use' ELSE 'not_found' END";

pub(super) async fn inference(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: &Uuid7,
) -> Result<Option<ModelInference>> {
    let value: Option<Option<sqlx::types::Json<ModelInference>>> =
        sqlx::query_scalar("SELECT inference FROM core.model_library WHERE id=$1::uuid FOR UPDATE")
            .bind(id.as_str())
            .fetch_optional(&mut **transaction)
            .await
            .map_err(query(CONTEXT_UPDATE))?;
    Ok(value.flatten().map(|value| value.0))
}
