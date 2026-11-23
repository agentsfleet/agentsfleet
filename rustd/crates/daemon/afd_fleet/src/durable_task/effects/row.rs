//! Checked storage projection; no arbitrary GitHub body enters a task receipt.

use crate::{
    Result,
    durable_task::{DurableTaskRefusal, github::BoundPr, row::counter},
};
use af_core::id::Uuid7;
use af_wire::durable_task::{DurableTaskEffect, DurableTaskEffectState};
use sqlx::types::Json;

#[derive(sqlx::FromRow)]
pub(super) struct Effect {
    pub id: Uuid7,
    pub durable_task_id: Uuid7,
    pub fleet_id: Uuid7,
    pub workspace_id: Uuid7,
    pub durable_task_generation: i64,
    pub stop_generation: i64,
    pub ordinal: i64,
    pub expected_head: String,
    pub tested_tree: String,
    pub request_digest: String,
    pub target: Option<Json<BoundPr>>,
    pub state: String,
    pub commit_oid: Option<String>,
}

impl Effect {
    pub(super) fn target(&self) -> Result<&BoundPr> {
        self.target
            .as_ref()
            .map(|target| &target.0)
            .ok_or_else(|| DurableTaskRefusal::NotFound.into())
    }

    pub(super) fn wire(&self) -> Result<DurableTaskEffect> {
        let state = match self.state.as_str() {
            "prepared" => DurableTaskEffectState::Prepared,
            "issued" => DurableTaskEffectState::Issued,
            "confirmed" => DurableTaskEffectState::Confirmed,
            "unknown" => DurableTaskEffectState::Unknown,
            "refused" => DurableTaskEffectState::Refused,
            _ => return Err(DurableTaskRefusal::EffectUncertain.into()),
        };
        Ok(DurableTaskEffect {
            effect_id: self.id.clone().into(),
            ordinal: counter(self.ordinal, "effect ordinal")?,
            state,
            expected_head: self.expected_head.clone(),
            tested_tree: self.tested_tree.clone(),
            commit: self.commit_oid.clone(),
            pull_request_url: self.target()?.url.clone(),
        })
    }
}
