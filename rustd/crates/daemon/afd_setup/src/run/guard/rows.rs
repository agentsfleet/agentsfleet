//! Decode durable authoring authority and its accepted policy together.

use super::Attempt;
use crate::{
    Result,
    draft::{value, word},
};
use af_core::id::Uuid7;
use jiff::Timestamp;
use std::time::Duration;

/// One `authoring.attempts` row, by column name.
#[derive(sqlx::FromRow)]
pub(crate) struct AttemptRow {
    id: Uuid7,
    workspace_id: Uuid7,
    tenant_id: Uuid7,
    user_id: Uuid7,
    draft_id: Uuid7,
    message_sequence: i64,
    fencing_token: i64,
    model_snapshot: String,
    status: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    run_expires_at: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    expires_at: Timestamp,
    last_activity_sequence: i64,
    model_token_hash: String,
    call_count: i32,
    output_token_count: i64,
    usage_unknown: bool,
    reserved_nanos: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    reported_runtime_ms: i64,
    failure_reason: Option<String>,
    #[sqlx(try_from = "i32")]
    max_model_calls: u32,
    #[sqlx(try_from = "i32")]
    max_wall_seconds: u32,
    #[sqlx(try_from = "i32")]
    max_proposal_corrections: u32,
    #[sqlx(try_from = "i32")]
    provider_timeout_seconds: u32,
}

impl AttemptRow {
    pub(crate) fn decode(self) -> Result<Attempt> {
        let settings = garde::Unvalidated::new(crate::turn_policy::TurnSettings {
            max_model_calls: self.max_model_calls,
            max_wall_seconds: self.max_wall_seconds,
            max_proposal_corrections: self.max_proposal_corrections,
            provider_timeout_seconds: self.provider_timeout_seconds,
        })
        .validate()
        .map_err(|source| crate::error::ErrorKind::Query {
            context: "decode saved authoring turn limits",
            source: sqlx::Error::Decode(Box::new(source)),
        })?;
        Ok(Attempt {
            policy: crate::turn_policy::TurnPolicy::new(&settings),
            id: self.id.clone(),
            workspace_id: self.workspace_id.clone(),
            tenant_id: self.tenant_id.clone(),
            user_id: self.user_id.clone(),
            draft_id: self.draft_id.clone(),
            message_sequence: self.message_sequence,
            fencing_token: self.fencing_token,
            model_snapshot: value(&self.model_snapshot)?,
            status: word(&self.status)?,
            run_expires_at: self.run_expires_at,
            expires_at: self.expires_at,
            last_activity_sequence: self.last_activity_sequence,
            model_token_hash: self.model_token_hash,
            call_count: self.call_count,
            output_token_count: self.output_token_count,
            usage_unknown: self.usage_unknown,
            reserved_nanos: self.reserved_nanos,
            created_at: self.created_at,
            reported_runtime: Duration::from_millis(
                u64::try_from(self.reported_runtime_ms).unwrap_or(0),
            ),
            failure_reason: self.failure_reason.as_deref().map(word).transpose()?,
        })
    }
}
