//! Checked storage projection; no arbitrary GitHub body enters a task receipt.

use crate::{
    Result,
    task::{TaskRefusal, github::BoundPr, row::counter},
};
use af_wire::task::{TaskEffect, TaskEffectState};
use sqlx::types::Json;

#[derive(sqlx::FromRow)]
pub(super) struct Effect {
    pub id: String,
    pub task_id: String,
    pub fleet_id: String,
    pub workspace_id: String,
    pub task_generation: i64,
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
            .ok_or_else(|| TaskRefusal::NotFound.into())
    }

    pub(super) fn wire(&self) -> Result<TaskEffect> {
        let state = match self.state.as_str() {
            "prepared" => TaskEffectState::Prepared,
            "issued" => TaskEffectState::Issued,
            "confirmed" => TaskEffectState::Confirmed,
            "unknown" => TaskEffectState::Unknown,
            "refused" => TaskEffectState::Refused,
            _ => return Err(TaskRefusal::EffectUncertain.into()),
        };
        Ok(TaskEffect {
            effect_id: self.id.clone(),
            ordinal: counter(self.ordinal, "effect ordinal")?,
            state,
            expected_head: self.expected_head.clone(),
            tested_tree: self.tested_tree.clone(),
            commit: self.commit_oid.clone(),
            pull_request_url: self.target()?.url.clone(),
        })
    }
}
