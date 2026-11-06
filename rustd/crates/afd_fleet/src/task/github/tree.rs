//! Reproduce supported changes as unreachable objects before advancing any ref.

use super::{
    BoundPr,
    http::{Session, path},
    types::{Commit, Object},
};
use crate::{Result, task::TaskRefusal};
use afd_wire::task::TaskEffectInput;
use base64::{Engine as _, prelude::BASE64_STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Tree {
    tree: Vec<Entry>,
    truncated: bool,
}
#[derive(Deserialize)]
struct Entry {
    path: String,
    mode: String,
    sha: String,
}

pub(super) async fn reproduce(
    session: &Session<'_>,
    target: &BoundPr,
    input: &garde::Valid<TaskEffectInput>,
) -> Result<()> {
    let (owner, name) = target
        .repository
        .split_once('/')
        .ok_or(TaskRefusal::InvalidTarget)?;
    let commit: Commit = session
        .get(&path(&[
            "repos",
            owner,
            name,
            "git",
            "commits",
            &input.expected_head,
        ])?)
        .await?;
    if commit.sha != input.expected_head {
        return Err(TaskRefusal::InvalidTarget.into());
    }
    let mut changes = Vec::with_capacity(input.changes.len());
    let mut bytes = 0_usize;
    for (index, change) in input.changes.iter().enumerate() {
        if input.changes.iter().take(index).any(|previous| {
            previous.path == change.path
                || previous.path.starts_with(&format!("{}/", change.path))
                || change.path.starts_with(&format!("{}/", previous.path))
        }) {
            return Err(TaskRefusal::InvalidTarget.into());
        }
        regular(
            session,
            owner,
            name,
            &commit.tree.sha,
            &change.path,
            change.contents.is_none(),
        )
        .await?;
        let sha = if let Some(content) = &change.contents {
            let decoded = BASE64_STANDARD
                .decode(content)
                .map_err(|_invalid| TaskRefusal::InvalidTarget)?;
            bytes = bytes.saturating_add(decoded.len());
            if bytes > 1024 * 1024 {
                return Err(TaskRefusal::LimitExhausted.into());
            }
            let blob: Object = session
                .post(
                    &path(&["repos", owner, name, "git", "blobs"])?,
                    &json!({"encoding":"base64","content":content}),
                )
                .await?;
            Value::String(blob.sha)
        } else {
            Value::Null
        };
        changes.push(json!({"path":change.path,"mode":"100644","type":"blob","sha":sha}));
    }
    let tree: Object = session
        .post(
            &path(&["repos", owner, name, "git", "trees"])?,
            &json!({"base_tree":commit.tree.sha,"tree":changes}),
        )
        .await?;
    if tree.sha != input.tested_tree || tree.sha == commit.tree.sha {
        return Err(TaskRefusal::InvalidTarget.into());
    }
    Ok(())
}

async fn regular(
    session: &Session<'_>,
    owner: &str,
    name: &str,
    root: &str,
    file: &str,
    deleting: bool,
) -> Result<()> {
    let mut tree = root.to_owned();
    let mut components = file.split('/').peekable();
    while let Some(component) = components.next() {
        let entries: Tree = session
            .get(&path(&["repos", owner, name, "git", "trees", &tree])?)
            .await?;
        if entries.truncated {
            return Err(TaskRefusal::InvalidTarget.into());
        }
        let Some(entry) = entries
            .tree
            .into_iter()
            .find(|entry| entry.path == component)
        else {
            return if deleting {
                Err(TaskRefusal::InvalidTarget.into())
            } else {
                Ok(())
            };
        };
        let expected = if components.peek().is_some() {
            "040000"
        } else {
            "100644"
        };
        if entry.mode != expected {
            return Err(TaskRefusal::InvalidTarget.into());
        }
        tree = entry.sha;
    }
    Ok(())
}
