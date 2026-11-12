//! One unsettled usage line as the database returns it, and how it becomes the
//! settlement a biller is asked to charge.

use af_core::id::Uuid7;
use af_wire::usage::{LineKind, UsageLine};
use jiff::Timestamp;

use crate::biller::{Account, Attribution, Settlement, WorkKey};
use crate::price::RatedLine;
use crate::rates::Posture;
use crate::{Nanos, sql};

/// One unsettled line, by column name.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct UnsettledRow {
    pub(super) work_type: String,
    pub(super) work_id: Uuid7,
    pub(super) kind: String,
    pub(super) seq: i64,
    pub(super) tenant_id: Uuid7,
    pub(super) workspace_id: Uuid7,
    pub(super) fleet_id: Option<Uuid7>,
    pub(super) fleet_name: Option<String>,
    pub(super) event_id: Option<Uuid7>,
    pub(super) provider: String,
    pub(super) model: String,
    pub(super) posture: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub(super) requested_at: Timestamp,
    pub(super) input_token_count: i64,
    pub(super) cached_input_token_count: i64,
    pub(super) cache_write_token_count: i64,
    pub(super) cache_write_5m_token_count: i64,
    pub(super) cache_write_1h_token_count: i64,
    pub(super) output_token_count: i64,
    pub(super) runtime_ms: i64,
    pub(super) price_id: Option<Uuid7>,
    pub(super) unpriced_token_count: i64,
    pub(super) charged_nanos: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub(super) logged_at: Timestamp,
    pub(super) line_id: Uuid7,
}

impl UnsettledRow {
    /// Money and attribution remain under the original work identity while
    /// independently configured providers retain their own settlement header.
    pub(super) fn identity(&self) -> (&str, &str, &str, &str, &str) {
        (
            &self.work_type,
            self.work_id.as_str(),
            &self.provider,
            &self.model,
            &self.posture,
        )
    }

    /// The row's line as the work priced it; `None` for a line kind this
    /// build cannot name.
    pub(super) fn line(&self) -> Option<RatedLine> {
        let kind = LineKind::parse(&self.kind)?;
        Some(RatedLine {
            usage: UsageLine {
                kind,
                seq: count(self.seq),
                at: self.requested_at,
                input_tokens: count(self.input_token_count),
                cached_input_tokens: count(self.cached_input_token_count),
                cache_write_tokens: count(self.cache_write_token_count),
                cache_write_5m_tokens: count(self.cache_write_5m_token_count),
                cache_write_1h_tokens: count(self.cache_write_1h_token_count),
                output_tokens: count(self.output_token_count),
                runtime_ms: count(self.runtime_ms),
            },
            price: self.price_id.clone(),
            charged: Nanos::from_i64(self.charged_nanos),
            unpriced_tokens: u64::try_from(self.unpriced_token_count).unwrap_or(0),
        })
    }

    /// The work this row belongs to, by its stored kind.
    pub(super) fn work(&self) -> Option<fn(Uuid7) -> WorkKey> {
        match self.work_type.as_str() {
            sql::work::FLEET_LEASE => Some(WorkKey::FleetLease),
            sql::work::AUTHORING_ATTEMPT => Some(WorkKey::AuthoringAttempt),
            _ => None,
        }
    }

    /// A settlement for this row's `work` under `posture`, retaining its
    /// complete priced group.
    pub(super) fn settlement(
        self,
        work: fn(Uuid7) -> WorkKey,
        posture: Posture,
        lines: Vec<RatedLine>,
    ) -> Settlement {
        let attribution = match (self.fleet_id, self.event_id) {
            (Some(fleet), Some(event_id)) => Attribution::Fleet {
                fleet,
                fleet_name: self.fleet_name.unwrap_or_default(),
                event_id,
            },
            _ => Attribution::SetupChat,
        };
        Settlement {
            work: work(self.work_id),
            payer: Account {
                tenant: self.tenant_id,
                workspace: self.workspace_id,
            },
            attribution,
            provider: self.provider,
            model: self.model,
            posture,
            lines,
        }
    }
}

/// A stored count as the line type holds it.
///
/// Every count was written from a `u32` and the table refuses negatives, so the
/// saturation never fires; it is here so a hand-edited row cannot panic a
/// sweeper.
fn count(stored: i64) -> u32 {
    u32::try_from(stored).unwrap_or(u32::MAX)
}
