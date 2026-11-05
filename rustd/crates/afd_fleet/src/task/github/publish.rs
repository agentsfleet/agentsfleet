//! One atomic expected-head commit, with history-based lost-ack reconciliation.

use super::{BoundPr, http::Session, tree};
use crate::{Result, task::TaskRefusal};
use afd_wire::task::TaskEffectInput;
use serde_json::{Value, json};

const MUTATION: &str = "mutation($input:CreateCommitOnBranchInput!){createCommitOnBranch(input:$input){commit{oid tree{oid} parents(first:2){nodes{oid}}}}}";
const HISTORY: &str = "query($owner:String!,$name:String!,$branch:String!){repository(owner:$owner,name:$name){databaseId ref(qualifiedName:$branch){target{...on Commit{history(first:50){nodes{oid message tree{oid} parents(first:2){nodes{oid}}}}}}}}}";

pub(in crate::task) enum Reconciled {
    Confirmed(String),
    Unknown,
}

impl Session<'_> {
    pub(in crate::task) async fn prepare_tree(
        &self,
        target: &BoundPr,
        input: &garde::Valid<TaskEffectInput>,
    ) -> Result<()> {
        if input.expected_head != target.head {
            return Err(TaskRefusal::StaleState.into());
        }
        target.verify(self).await?;
        tree::reproduce(self, target, input).await
    }

    pub(in crate::task) async fn publish(
        self,
        permit: crate::task::effects::PublicationPermit<'_>,
    ) -> Result<String> {
        let (effect, target, input) = permit.into_parts();
        if input.expected_head != target.head {
            return Err(TaskRefusal::StaleState.into());
        }
        let additions: Vec<_> = input
            .changes
            .iter()
            .filter_map(|file| {
                file.contents
                    .as_ref()
                    .map(|contents| json!({"path":file.path,"contents":contents}))
            })
            .collect();
        let deletions: Vec<_> = input
            .changes
            .iter()
            .filter(|file| file.contents.is_none())
            .map(|file| json!({"path":file.path}))
            .collect();
        let response: Value = self.post("graphql", &json!({"query":MUTATION,"variables":{"input":{
            "branch":{"repositoryNameWithOwner":target.repository,"branchName":target.branch},
            "expectedHeadOid":input.expected_head,"clientMutationId":effect,
            "message":{"headline":"Continue the assigned repair","body":marker(&effect)},
            "fileChanges":{"additions":additions,"deletions":deletions}
        }}})).await?;
        let commit = response
            .pointer("/data/createCommitOnBranch/commit")
            .ok_or(TaskRefusal::EffectUncertain)?;
        if response.get("errors").is_some()
            || !matches(commit, &input.expected_head, &input.tested_tree)
        {
            return Err(TaskRefusal::EffectUncertain.into());
        }
        commit
            .get("oid")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| TaskRefusal::EffectUncertain.into())
    }

    pub(in crate::task) async fn reconcile(
        &self,
        target: &BoundPr,
        effect: &str,
        head: &str,
        tree: &str,
    ) -> Result<Reconciled> {
        self.reconcile_locator(
            &target.repository,
            &target.branch,
            target.repository_id,
            target.installation_id,
            effect,
            head,
            tree,
        )
        .await
    }

    pub(in crate::task) async fn reconcile_locator(
        &self,
        repository: &str,
        branch: &str,
        repository_id: u64,
        installation_id: u64,
        effect: &str,
        head: &str,
        tree: &str,
    ) -> Result<Reconciled> {
        if self.installation_id != installation_id {
            return Err(TaskRefusal::InvalidTarget.into());
        }
        let (owner, name) = repository
            .split_once('/')
            .ok_or(TaskRefusal::InvalidTarget)?;
        let response: Value = self
            .post(
                "graphql",
                &json!({"query":HISTORY,"variables":{
                    "owner":owner,"name":name,"branch":format!("refs/heads/{branch}")
                }}),
            )
            .await?;
        if response.get("errors").is_some()
            || response
                .pointer("/data/repository/databaseId")
                .and_then(Value::as_u64)
                != Some(repository_id)
        {
            return Err(TaskRefusal::EffectUncertain.into());
        }
        let history = response
            .pointer("/data/repository/ref/target/history/nodes")
            .and_then(Value::as_array)
            .ok_or(TaskRefusal::EffectUncertain)?;
        for commit in history {
            if commit
                .get("message")
                .and_then(Value::as_str)
                .is_some_and(|message| message.lines().any(|line| line == marker(effect)))
            {
                if !matches(commit, head, tree) {
                    return Ok(Reconciled::Unknown);
                }
                return Ok(Reconciled::Confirmed(
                    commit
                        .get("oid")
                        .and_then(Value::as_str)
                        .ok_or(TaskRefusal::EffectUncertain)?
                        .to_owned(),
                ));
            }
        }
        // Absence cannot prove non-publication: a human may already have
        // force-pushed away a successful commit. Never authorize a blind retry.
        Ok(Reconciled::Unknown)
    }
}

fn marker(effect: &str) -> String {
    format!("agentsfleet-effect:{effect}")
}

fn matches(commit: &Value, parent: &str, tree: &str) -> bool {
    commit.pointer("/tree/oid").and_then(Value::as_str) == Some(tree)
        && commit
            .pointer("/parents/nodes")
            .and_then(Value::as_array)
            .is_some_and(|parents| {
                parents.len() == 1 && parents[0].get("oid").and_then(Value::as_str) == Some(parent)
            })
}
