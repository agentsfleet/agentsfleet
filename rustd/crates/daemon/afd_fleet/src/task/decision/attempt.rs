//! An eligible retry reuses the original request, quote and logical deadline.

use af_core::id::Uuid7;
use af_wire::{
    decision::DecisionRequest,
    task::{DecisionCallState, DecisionRetryDelay, DecisionUnavailableCause},
};
use sqlx::PgConnection;

use super::{
    prepare::Prepared,
    quote::FrozenQuote,
    row::{self, Call, Lookup},
};
use crate::{Result, task::TaskRefusal};

pub(super) enum Attempt {
    Original {
        assessment_id: String,
    },
    Retry {
        parent: Box<Call>,
        request: DecisionRequest,
    },
}

impl Attempt {
    pub(super) async fn resolve(
        connection: &mut PgConnection,
        prepared: &Prepared,
        parent: Option<&str>,
        key: &str,
        digest: &str,
        call: &Uuid7,
        assessment_id: String,
    ) -> Result<Self> {
        let Some(parent) = parent else {
            return Ok(Self::Original { assessment_id });
        };
        let parent = row::find(connection, Lookup::Id(parent))
            .await?
            .ok_or(TaskRefusal::NotFound)?;
        if parent.attempt_number != 0
            || !parent.issued
            || parent.assessment_key != key
            || parent.request_digest != digest
            || parent.fleet_id != prepared.current.fleet_id
            || parent.workspace_id != prepared.current.workspace_id
            || parent.logical_expires_at <= prepared.current.checked_at
            || parent.state()? != DecisionCallState::Unknown
        {
            return Err(TaskRefusal::ReplayConflict.into());
        }
        let delay = match parent.failure_reason.as_ref().map(|reason| &reason.0) {
            Some(DecisionUnavailableCause::ProviderStatus {
                status: 429 | 529,
                retry_delay,
            }) => match retry_delay {
                DecisionRetryDelay::Unspecified => 0,
                DecisionRetryDelay::WithinBudget { milliseconds } => i64::from(*milliseconds),
                DecisionRetryDelay::TooLong => return Err(TaskRefusal::DecisionUnavailable.into()),
            },
            _ => return Err(TaskRefusal::DecisionUnavailable.into()),
        };
        if parent.failure_age_ms.is_none_or(|elapsed| elapsed < delay) {
            return Err(TaskRefusal::DecisionUnavailable.into());
        }
        let payload = row::payload(connection, &parent.id)
            .await?
            .ok_or(TaskRefusal::DecisionUnavailable)?;
        let mut request = payload.request.0;
        request.call_id = call.to_string();
        Ok(Self::Retry {
            parent: Box::new(parent),
            request,
        })
    }

    pub(super) fn number(&self) -> i16 {
        match self {
            Self::Original { .. } => 0,
            Self::Retry { .. } => 1,
        }
    }

    pub(super) fn assessment_id(&self) -> &str {
        match self {
            Self::Original { assessment_id } => assessment_id,
            Self::Retry { parent, .. } => &parent.assessment_id,
        }
    }

    pub(super) fn parent(&self) -> Option<&Call> {
        match self {
            Self::Original { .. } => None,
            Self::Retry { parent, .. } => Some(parent),
        }
    }

    pub(super) fn quote<'a>(&'a self, initial: &'a FrozenQuote) -> &'a FrozenQuote {
        match self {
            Self::Original { .. } => initial,
            Self::Retry { parent, .. } => &parent.price_snapshot.0,
        }
    }

    pub(super) fn request<'a>(&'a self, initial: &'a DecisionRequest) -> &'a DecisionRequest {
        match self {
            Self::Original { .. } => initial,
            Self::Retry { request, .. } => request,
        }
    }
}
