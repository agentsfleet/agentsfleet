//! Targets come from an exact human-assigned PR under accepted and live setup.

use super::{
    BoundPr, Github,
    http::{Session, path},
    types::Object,
    workflow,
};
use crate::{
    Result,
    task::{TaskLease, TaskRefusal},
};
use afd_fleet_runtime::{
    FleetConfig,
    config::{Access, RepositoryPublication},
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Repository {
    id: u64,
    full_name: String,
    default_branch: String,
}
#[derive(Deserialize)]
struct Endpoint {
    #[serde(rename = "ref")]
    name: String,
    sha: String,
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
struct Branch {
    protected: bool,
    commit: Object,
}

impl Github {
    pub(in crate::task) async fn prepare(
        &self,
        lease: TaskLease<'_>,
        config: &FleetConfig,
        repository: &str,
        number: u32,
        human_message: &str,
    ) -> Result<BoundPr> {
        let binding = config
            .repository_binding()
            .ok_or(TaskRefusal::InvalidTarget)?;
        if binding.publication() != RepositoryPublication::ExistingPr
            || binding.access() != Access::Write
            || !binding
                .repositories()
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(repository))
        {
            return Err(TaskRefusal::InvalidTarget.into());
        }
        let url = format!("https://github.com/{repository}/pull/{number}");
        if !human_message.split_whitespace().any(|word| {
            word.trim_matches(|c: char| matches!(c, '(' | ')' | '<' | '>' | ',' | '.')) == url
        }) {
            return Err(TaskRefusal::InvalidTarget.into());
        }
        let session = self.read_session(lease, repository).await?;
        let (owner, name) = repository
            .split_once('/')
            .ok_or(TaskRefusal::InvalidTarget)?;
        let pr: PullRequest = session
            .get(&path(&[
                "repos",
                owner,
                name,
                "pulls",
                &number.to_string(),
            ])?)
            .await?;
        check(
            &pr,
            repository,
            binding.reach().base().ok_or(TaskRefusal::InvalidTarget)?,
        )?;
        let branch: Branch = session
            .get(&path(&["repos", owner, name, "branches", &pr.head.name])?)
            .await?;
        if branch.protected || branch.commit.sha != pr.head.sha {
            return Err(TaskRefusal::InvalidTarget.into());
        }
        let commit: super::types::Commit = session
            .get(&path(&[
                "repos",
                owner,
                name,
                "git",
                "commits",
                &pr.head.sha,
            ])?)
            .await?;
        if commit.sha != pr.head.sha {
            return Err(TaskRefusal::InvalidTarget.into());
        }
        let workflows = workflow::review(
            &session,
            owner,
            name,
            &pr.head.sha,
            &pr.base.repo.default_branch,
            binding.validation_workflows(),
        )
        .await?;
        Ok(BoundPr {
            repository: pr.base.repo.full_name,
            repository_id: pr.base.repo.id,
            installation_id: session.installation_id,
            number,
            branch: pr.head.name,
            base: pr.base.name,
            head: pr.head.sha,
            tree: commit.tree.sha,
            url,
            workflows,
        })
    }
}

fn check(pr: &PullRequest, repository: &str, base: &str) -> Result<()> {
    if pr.number == 0
        || pr.state != "open"
        || pr.merged
        || pr.head.repo.id != pr.base.repo.id
        || !pr.base.repo.full_name.eq_ignore_ascii_case(repository)
        || pr.base.name != base
        || pr.head.name == pr.base.name
        || pr.head.name == pr.base.repo.default_branch
    {
        return Err(TaskRefusal::InvalidTarget.into());
    }
    Ok(())
}

impl BoundPr {
    pub(in crate::task) fn within(&self, config: &FleetConfig) -> Result<()> {
        let binding = config
            .repository_binding()
            .ok_or(TaskRefusal::InvalidTarget)?;
        if binding.publication() != RepositoryPublication::ExistingPr
            || binding.access() != Access::Write
            || binding.reach().base() != Some(self.base.as_str())
            || !binding
                .repositories()
                .iter()
                .any(|repository| repository.eq_ignore_ascii_case(&self.repository))
            || binding.validation_workflows().len() != self.workflows.len()
            || !self.workflows.iter().all(|workflow| {
                binding.validation_workflows().iter().any(|accepted| {
                    accepted.path == workflow.path && accepted.sha256 == workflow.digest
                })
            })
        {
            return Err(TaskRefusal::InvalidTarget.into());
        }
        Ok(())
    }

    pub(super) async fn verify(&self, session: &Session<'_>) -> Result<()> {
        let (owner, name) = self
            .repository
            .split_once('/')
            .ok_or(TaskRefusal::InvalidTarget)?;
        let pr: PullRequest = session
            .get(&path(&[
                "repos",
                owner,
                name,
                "pulls",
                &self.number.to_string(),
            ])?)
            .await?;
        check(&pr, &self.repository, &self.base)?;
        let branch: Branch = session
            .get(&path(&["repos", owner, name, "branches", &self.branch])?)
            .await?;
        if pr.head.name != self.branch
            || pr.base.repo.id != self.repository_id
            || session.installation_id != self.installation_id
            || branch.protected
            || pr.head.sha != self.head
            || branch.commit.sha != self.head
        {
            return Err(TaskRefusal::StaleState.into());
        }
        workflow::verify(session, owner, name, &self.head, &self.workflows).await?;
        workflow::verify_default(
            session,
            owner,
            name,
            &pr.base.repo.default_branch,
            &self.workflows,
        )
        .await
    }
}
