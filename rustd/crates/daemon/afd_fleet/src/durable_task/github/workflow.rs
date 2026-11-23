//! Owner-reviewed content plus native rejection of privileged workflow shapes.

use super::{
    Workflow,
    http::{Session, path},
};
use crate::{Result, durable_task::DurableTaskRefusal};
use af_fleet_runtime::config::ValidationWorkflow;
use base64::{Engine as _, prelude::BASE64_STANDARD};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct File {
    path: String,
    sha: String,
    #[serde(rename = "type")]
    kind: String,
}
#[derive(Deserialize)]
struct Blob {
    content: String,
    encoding: String,
}
#[derive(Deserialize)]
struct Workflows {
    total_count: u64,
    workflows: Vec<Listed>,
}
#[derive(Deserialize)]
struct Listed {
    id: u64,
    path: String,
    state: String,
}

pub(super) async fn review(
    session: &Session<'_>,
    owner: &str,
    name: &str,
    head: &str,
    default_branch: &str,
    reviewed: &[ValidationWorkflow],
) -> Result<Vec<Workflow>> {
    let listed: Workflows = session
        .get(&format!(
            "{}?per_page=17",
            path(&["repos", owner, name, "actions", "workflows"])?
        ))
        .await?;
    if listed.total_count != u64::try_from(reviewed.len()).unwrap_or(u64::MAX)
        || listed.workflows.len() != reviewed.len()
    {
        return Err(DurableTaskRefusal::InvalidTarget.into());
    }
    let mut workflows = Vec::with_capacity(reviewed.len());
    for accepted in reviewed {
        let workflow = listed
            .workflows
            .iter()
            .find(|item| item.path == accepted.path && item.state == "active")
            .ok_or(DurableTaskRefusal::InvalidTarget)?;
        workflows.push(Workflow {
            id: workflow.id,
            path: accepted.path.clone(),
            digest: accepted.sha256.clone(),
        });
    }
    verify(session, owner, name, head, &workflows).await?;
    verify_default(session, owner, name, default_branch, &workflows).await?;
    Ok(workflows)
}

pub(super) async fn verify_default(
    session: &Session<'_>,
    owner: &str,
    name: &str,
    branch: &str,
    workflows: &[Workflow],
) -> Result<()> {
    #[derive(Deserialize)]
    struct Branch {
        commit: super::types::Object,
    }
    let default: Branch = session
        .get(&path(&["repos", owner, name, "branches", branch])?)
        .await?;
    verify(session, owner, name, &default.commit.sha, workflows).await
}

pub(super) async fn verify(
    session: &Session<'_>,
    owner: &str,
    name: &str,
    head: &str,
    workflows: &[Workflow],
) -> Result<()> {
    let files: Vec<File> = session
        .get(&format!(
            "{}?ref={head}",
            path(&["repos", owner, name, "contents", ".github", "workflows"])?
        ))
        .await?;
    if workflows.is_empty() || workflows.len() > 16 || files.len() != workflows.len() {
        return Err(DurableTaskRefusal::InvalidTarget.into());
    }
    for file in files {
        let accepted = workflows
            .iter()
            .find(|workflow| workflow.path == file.path)
            .ok_or(DurableTaskRefusal::InvalidTarget)?;
        if file.kind != "file" {
            return Err(DurableTaskRefusal::InvalidTarget.into());
        }
        let blob: Blob = session
            .get(&path(&["repos", owner, name, "git", "blobs", &file.sha])?)
            .await?;
        if blob.encoding != "base64" {
            return Err(DurableTaskRefusal::InvalidTarget.into());
        }
        let bytes = BASE64_STANDARD
            .decode(blob.content.replace('\n', ""))
            .map_err(|_invalid| DurableTaskRefusal::InvalidTarget)?;
        if bytes.len() > 1024 * 1024 || af_core::etag::sha256_hex(&bytes) != accepted.digest {
            return Err(DurableTaskRefusal::InvalidTarget.into());
        }
        let text =
            std::str::from_utf8(&bytes).map_err(|_invalid| DurableTaskRefusal::InvalidTarget)?;
        safe(text)?;
    }
    Ok(())
}

fn safe(text: &str) -> Result<()> {
    let options = serde_saphyr::options! { budget: serde_saphyr::budget! {
        max_documents: 1, max_nodes: 16_384, max_events: 32_768, max_depth: 32,
        max_aliases: 0, max_anchors: 0, max_merge_keys: 0, max_total_scalar_bytes: 1024 * 1024,
    } };
    let document: Value = serde_saphyr::from_str_with_options(text, options)
        .map_err(|_invalid| DurableTaskRefusal::InvalidTarget)?;
    let jobs = document
        .get("jobs")
        .and_then(Value::as_object)
        .ok_or(DurableTaskRefusal::InvalidTarget)?;
    if !read_permissions(document.get("permissions"))
        || jobs.is_empty()
        || jobs.len() > 32
        || has_secrets(&document)
        || document.get("on").is_none()
    {
        return Err(DurableTaskRefusal::InvalidTarget.into());
    }
    for job in jobs.values() {
        if job.get("environment").is_some()
            || job.get("secrets").is_some()
            || job.get("uses").is_some()
            || !job
                .get("runs-on")
                .and_then(Value::as_str)
                .is_some_and(|host| {
                    matches!(
                        host,
                        "ubuntu-latest"
                            | "ubuntu-24.04"
                            | "ubuntu-22.04"
                            | "macos-latest"
                            | "windows-latest"
                    )
                })
            || job
                .get("permissions")
                .is_some_and(|value| !read_permissions(Some(value)))
        {
            return Err(DurableTaskRefusal::InvalidTarget.into());
        }
    }
    Ok(())
}

fn has_secrets(value: &Value) -> bool {
    match value {
        Value::String(text) => text
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .any(|identifier| identifier.eq_ignore_ascii_case("secrets")),
        Value::Array(values) => values.iter().any(has_secrets),
        Value::Object(values) => values
            .iter()
            .any(|(key, value)| key.eq_ignore_ascii_case("secrets") || has_secrets(value)),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

fn read_permissions(value: Option<&Value>) -> bool {
    value.is_some_and(|value| {
        value.as_str() == Some("read-all")
            || value.as_object().is_some_and(|map| {
                map.values()
                    .all(|permission| matches!(permission.as_str(), Some("read" | "none")))
            })
    })
}

#[cfg(test)]
#[path = "workflow_tests.rs"]
mod tests;
