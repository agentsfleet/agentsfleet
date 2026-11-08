//! Independent exact-head evidence, fetched without a sandbox or decision call.

use super::{
    BoundPr,
    http::{Session, path},
};
use crate::{Result, task::TaskRefusal};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(in crate::task) use af_wire::task::TaskCheck as Check;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::task) struct Snapshot {
    pub open: bool,
    pub head: String,
    pub checks: Vec<Check>,
}

impl Snapshot {
    pub(in crate::task) fn checks_finished(&self, target: &BoundPr) -> bool {
        self.open
            && self.head == target.head
            && !self.checks.is_empty()
            && self.checks.len() == target.workflows.len()
            && self.checks.iter().all(|check| check.status == "completed")
    }

    pub(in crate::task) fn digest(&self) -> Result<String> {
        Ok(af_core::etag::sha256_hex(
            &serde_json::to_vec(self).map_err(af_core::json::Unreadable::from)?,
        ))
    }
    pub(in crate::task) fn green(&self, target: &BoundPr) -> bool {
        self.open
            && self.head == target.head
            && !self.checks.is_empty()
            && self.checks.len() == target.workflows.len()
            && self.checks.iter().all(|check| {
                check.status == "completed" && check.conclusion.as_deref() == Some("success")
            })
    }
}

#[derive(Deserialize)]
struct Repository {
    id: u64,
    full_name: String,
}
#[derive(Deserialize)]
struct Endpoint {
    sha: String,
    #[serde(rename = "ref")]
    branch: String,
    repo: Repository,
}
#[derive(Deserialize)]
struct PullRequest {
    number: u32,
    state: String,
    merged: bool,
    head: Endpoint,
    base: Endpoint,
}
#[derive(Deserialize)]
struct Runs {
    total_count: u64,
    workflow_runs: Vec<Execution>,
}
#[derive(Deserialize)]
struct Execution {
    id: u64,
    workflow_id: u64,
    run_attempt: u32,
    run_number: u64,
    head_sha: String,
    status: String,
    conclusion: Option<String>,
}

impl Session<'_> {
    /// Two bounded read-only requests form one authoritative dependency observation.
    pub(in crate::task) async fn status(&self, target: &BoundPr) -> Result<Snapshot> {
        if self.installation_id != target.installation_id {
            return Err(TaskRefusal::InvalidTarget.into());
        }
        self.read_credential()?;
        let (owner, name) = target
            .repository
            .split_once('/')
            .ok_or(TaskRefusal::InvalidTarget)?;
        let pr: PullRequest = self
            .get(&path(&[
                "repos",
                owner,
                name,
                "pulls",
                &target.number.to_string(),
            ])?)
            .await?;
        if pr.number != target.number
            || pr.base.repo.id != target.repository_id
            || pr.head.repo.id != target.repository_id
            || !pr
                .base
                .repo
                .full_name
                .eq_ignore_ascii_case(&target.repository)
            || pr.head.branch != target.branch
            || pr.base.branch != target.base
        {
            return Err(TaskRefusal::InvalidTarget.into());
        }
        let open = pr.state == "open" && !pr.merged;
        if !open || pr.head.sha != target.head {
            return Ok(Snapshot {
                open,
                head: pr.head.sha,
                checks: Vec::new(),
            });
        }
        let runs: Runs = self
            .get(&format!(
                "{}?head_sha={}&per_page=100",
                path(&["repos", owner, name, "actions", "runs"])?,
                target.head
            ))
            .await?;
        if runs.total_count > 100
            || u64::try_from(runs.workflow_runs.len()).unwrap_or(u64::MAX) != runs.total_count
        {
            return Err(TaskRefusal::InvalidTarget.into());
        }
        let mut latest = BTreeMap::<u64, &Execution>::new();
        for run in &runs.workflow_runs {
            if run.head_sha != target.head || run.id == 0 || run.run_attempt == 0 {
                return Err(TaskRefusal::InvalidTarget.into());
            }
            let entry = latest.entry(run.workflow_id).or_insert(run);
            if (run.run_number, run.run_attempt, run.id)
                > (entry.run_number, entry.run_attempt, entry.id)
            {
                *entry = run;
            }
        }
        let checks = target
            .workflows
            .iter()
            .map(|workflow| match latest.get(&workflow.id) {
                Some(run) => Check {
                    workflow_id: workflow.id,
                    run_id: Some(run.id),
                    run_number: Some(run.run_number),
                    run_attempt: Some(run.run_attempt),
                    status: run.status.clone(),
                    conclusion: run.conclusion.clone(),
                },
                None => Check {
                    workflow_id: workflow.id,
                    run_id: None,
                    run_number: None,
                    run_attempt: None,
                    status: "pending".to_owned(),
                    conclusion: None,
                },
            })
            .collect();
        Ok(Snapshot {
            open,
            head: pr.head.sha,
            checks,
        })
    }
}
