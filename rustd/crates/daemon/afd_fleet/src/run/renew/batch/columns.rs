//! The batch statement's parallel arrays, and the order they are bound in.

use std::sync::Arc;

use af_core::id::Uuid7;
use af_wire::report::RenewRequest;
use af_wire::usage::LineKind;
use afd_billing::price::RatedLine;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use super::rows::Found;
use super::verdict::Judged;
use crate::error::Result;
use crate::run::sql;
use crate::run::usage::Pricing;

/// A run bound for the statement, with what it was priced and judged under.
pub(super) struct Bound<'a> {
    pub(super) found: &'a Found,
    pub(super) request: &'a RenewRequest,
    pub(super) pricing: Arc<Pricing>,
}

/// The statement's parallel arrays: one entry per run, one per request line.
pub(super) struct Columns<'a> {
    ids: Vec<&'a Uuid7>,
    posture: Vec<&'static str>,
    provider: Vec<&'a str>,
    model: Vec<&'a str>,
    catalogue: Vec<Option<i64>>,
    runtime_price: Vec<Option<Uuid7>>,
    runtime_line: Vec<Uuid7>,
    lines: Vec<(&'a Uuid7, Uuid7, RatedLine)>,
}

type Query<'a> = sqlx::query::QueryAs<'a, sqlx::Postgres, Judged, sqlx::postgres::PgArguments>;

impl<'a> Columns<'a> {
    /// Prices each run's request lines and mints every id the statement writes.
    pub(super) fn of(
        bound: &'a [Bound<'a>],
        entropy: &af_crypto::entropy::Entropy,
        now: Timestamp,
    ) -> Result<Self> {
        let mut columns = Self {
            ids: Vec::new(),
            posture: Vec::new(),
            provider: Vec::new(),
            model: Vec::new(),
            catalogue: Vec::new(),
            runtime_price: Vec::new(),
            runtime_line: Vec::new(),
            lines: Vec::new(),
        };
        for entry in bound {
            let row = entry.found;
            columns.ids.push(&row.id);
            columns.posture.push(entry.pricing.posture().as_str());
            columns.provider.push(entry.pricing.provider());
            columns.model.push(entry.pricing.model());
            columns.catalogue.push(entry.pricing.catalogue_version());
            columns.runtime_price.push(entry.pricing.runtime_price(now));
            columns.runtime_line.push(entropy.uuid7(now)?);
            for line in
                entry
                    .pricing
                    .request_lines(row.created_at, now, &entry.request.requests.0)?
            {
                columns.lines.push((&row.id, entropy.uuid7(now)?, line));
            }
        }
        Ok(columns)
    }

    fn line_column(&self, field: fn(&RatedLine) -> i64) -> Vec<i64> {
        self.lines
            .iter()
            .map(|(_run, _id, line)| field(line))
            .collect()
    }

    /// [`sql::renew::RENEW_BATCH`] with every parameter bound, in `$n` order.
    pub(super) fn bind(
        &self,
        runner_id: &'a Uuid7,
        clock: afd_runner::timing::RunClock,
    ) -> Query<'a> {
        sqlx::query_as(sql::renew::RENEW_BATCH)
            .bind(runner_id)
            .bind(self.ids.clone())
            .bind(self.posture.clone())
            .bind(self.provider.clone())
            .bind(self.model.clone())
            .bind(self.catalogue.clone())
            .bind(self.runtime_price.clone())
            .bind(self.runtime_line.clone())
            .bind(
                self.lines
                    .iter()
                    .map(|(run, _id, _line)| *run)
                    .collect::<Vec<_>>(),
            )
            .bind(
                self.lines
                    .iter()
                    .map(|(_run, id, _line)| id.clone())
                    .collect::<Vec<_>>(),
            )
            .bind(self.line_column(|line| i64::from(line.usage.seq)))
            .bind(
                self.lines
                    .iter()
                    .map(|(_run, _id, line)| line.usage.at.to_sqlx())
                    .collect::<Vec<_>>(),
            )
            .bind(self.line_column(|line| i64::from(line.usage.input_tokens)))
            .bind(self.line_column(|line| i64::from(line.usage.cached_input_tokens)))
            .bind(self.line_column(|line| i64::from(line.usage.cache_write_tokens)))
            .bind(self.line_column(|line| i64::from(line.usage.cache_write_5m_tokens)))
            .bind(self.line_column(|line| i64::from(line.usage.cache_write_1h_tokens)))
            .bind(self.line_column(|line| i64::from(line.usage.output_tokens)))
            .bind(
                self.lines
                    .iter()
                    .map(|(_run, _id, line)| line.price.clone())
                    .collect::<Vec<_>>(),
            )
            .bind(self.line_column(|line| i64::try_from(line.unpriced_tokens).unwrap_or(i64::MAX)))
            .bind(self.line_column(|line| line.charged.as_i64()))
            .bind(sql::RUN_STATUS_ACTIVE)
            .bind(afd_billing::nanos::MS_PER_SEC)
            .bind(afd_billing::RUN_NANOS_PER_SEC)
            .bind(clock.ttl())
            .bind(afd_billing::sql::work::FLEET_RUN)
            .bind(LineKind::Model.as_str())
            .bind(LineKind::Runtime.as_str())
            // The wire's runtime field is a `u32` of milliseconds.
            .bind(i64::from(u32::MAX))
            .bind(afd_runner::capacity::renewable_states())
    }
}
