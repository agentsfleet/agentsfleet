//! Wait claims use native task stamps, not completion payload assertions.

use super::super::{
    TaskScope,
    github::{BoundPr, status::Check},
};
use af_core::id::Uuid7;
use sqlx::types::Json;

#[derive(sqlx::FromRow)]
pub(super) struct Candidate {
    pub id: Uuid7,
    pub task_id: Uuid7,
    pub fleet_id: Uuid7,
    pub workspace_id: Uuid7,
}

impl Candidate {
    pub(super) fn scope(&self) -> TaskScope {
        TaskScope {
            fleet_id: self.fleet_id.clone(),
            workspace_id: self.workspace_id.clone(),
        }
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct Wait {
    pub id: Uuid7,
    pub task_id: Uuid7,
    pub fleet_id: Uuid7,
    pub workspace_id: Uuid7,
    pub task_generation: i64,
    pub stop_generation: i64,
    pub installation_id: i64,
    pub repository_id: i64,
    pub pull_request_number: i32,
    pub expected_head: String,
    pub checks: Json<Vec<Check>>,
    pub baseline_digest: String,
    pub fallback_count: i32,
    pub initial_read_at: Option<jiff_sqlx::Timestamp>,
    pub signalled_at: Option<jiff_sqlx::Timestamp>,
    pub claim_id: Option<Uuid7>,
    pub claim_valid: Option<bool>,
    pub signal_sequence: i64,
    pub observed_signal_sequence: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub expires_at: jiff::Timestamp,
    pub status: String,
    pub live_generation: i64,
    pub live_stop_generation: i64,
    pub accepted_revision_id: Uuid7,
    pub target: Json<BoundPr>,
    pub used_fallback_reads: i32,
    pub max_fallback_reads: i32,
    pub reply_provider: Option<String>,
    pub reply_address: Option<String>,
    pub source_admission_id: Uuid7,
    pub within_limits: bool,
    pub due: bool,
}

impl Wait {
    pub(super) fn current(&self) -> bool {
        self.status == af_wire::task::TaskState::Waiting.as_str()
            && self.task_generation == self.live_generation
            && self.stop_generation == self.live_stop_generation
            && self.target.installation_id == self.installation_id.unsigned_abs()
            && self.target.repository_id == self.repository_id.unsigned_abs()
            && self.target.number == self.pull_request_number.unsigned_abs()
            && self.target.head == self.expected_head
    }
    pub(super) const fn signalled(&self) -> bool {
        self.signal_sequence > self.observed_signal_sequence
    }
    pub(super) const fn fallback(&self) -> bool {
        self.initial_read_at.is_some() && !self.signalled()
    }
}
