//! Snapshot-bound keyset pages for the shared admin catalogue.
use crate::catalogue::{ModelRow, PriceProtocol, prices_for};
use crate::error::{Result, query};
use crate::{Model, Models, sql};
use afd_core::paging::{MAX_LIMIT, struct_cursor};
use serde::{Deserialize, Serialize};
use sqlx::Connection as _;
const CONTEXT_LIST: &str = "list admin models";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Boundary {
    v: u8,
    catalogue_version: i64,
    limit: u32,
    provider: String,
    model: String,
}
impl struct_cursor::StructCursor for Boundary {
    fn generation(&self) -> u8 {
        self.v
    }
}
/// One bounded, consistently priced catalogue page.
#[derive(Debug)]
pub struct ModelPage {
    /// Models in immutable provider/model identity order.
    pub models: Vec<Model>,
    /// Continue the same catalogue generation, or none at its end.
    pub next_cursor: Option<String>,
}
impl Models {
    /// Reads a bounded page. A catalogue edit invalidates continuation tokens.
    ///
    /// # Errors
    /// Refuses invalid bounds, malformed/mismatched/stale cursors and datastore failures.
    pub async fn page(&self, limit: u32, starting_after: Option<&str>) -> Result<ModelPage> {
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(crate::error::page());
        }
        let cursor = starting_after
            .map(struct_cursor::parse::<Boundary>)
            .transpose()
            .map_err(|_invalid| crate::error::page())?;
        if cursor.as_ref().is_some_and(|cursor| cursor.limit != limit) {
            return Err(crate::error::page());
        }
        let (mut models, version) = self
            .read(Some(i64::from(limit) + 1), cursor.as_ref())
            .await?;
        let more = models.len() > limit as usize;
        models.truncate(limit as usize);
        let next_cursor = models.last().filter(|_| more).map(|model| {
            struct_cursor::render(&Boundary {
                v: struct_cursor::VERSION,
                catalogue_version: version,
                limit,
                provider: model.provider.clone(),
                model: model.model_name.clone(),
            })
        });
        Ok(ModelPage {
            models,
            next_cursor,
        })
    }
    pub(crate) async fn read(
        &self,
        limit: Option<i64>,
        cursor: Option<&Boundary>,
    ) -> Result<(Vec<Model>, i64)> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection
            .as_mut()
            .begin_with(sql::BEGIN_MODEL_SNAPSHOT)
            .await
            .map_err(query(CONTEXT_LIST))?;
        let version: i64 = sqlx::query_scalar(include_str!("sql/read_catalogue_version.sql"))
            .fetch_one(&mut *transaction)
            .await
            .map_err(query(CONTEXT_LIST))?;
        if cursor.is_some_and(|cursor| cursor.catalogue_version != version) {
            return Err(crate::error::page());
        }
        let rows: Vec<ModelRow> = sqlx::query_as(sql::LIST_PLATFORM_MODELS)
            .bind(limit)
            .bind(cursor.map(|cursor| cursor.provider.as_str()))
            .bind(cursor.map(|cursor| cursor.model.as_str()))
            .fetch_all(&mut *transaction)
            .await
            .map_err(query(CONTEXT_LIST))?;
        let ids: Vec<&str> = rows.iter().map(|model| model.id.as_str()).collect();
        let mut prices = prices_for(&mut *transaction, &ids, PriceProtocol::Coding)
            .await
            .map_err(query(CONTEXT_LIST))?;
        let mut decision_prices = prices_for(&mut *transaction, &ids, PriceProtocol::Decision)
            .await
            .map_err(query(CONTEXT_LIST))?;
        transaction.commit().await.map_err(query(CONTEXT_LIST))?;
        let models = rows
            .into_iter()
            .map(|model| {
                let schedule = prices.take(&model.id);
                let decisions = decision_prices.take(&model.id);
                model.parse(schedule, decisions)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((models, version))
    }
}
