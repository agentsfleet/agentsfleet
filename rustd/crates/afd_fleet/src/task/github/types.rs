//! Persisted native target and validation identities contain no credential.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Workflow {
    pub id: u64,
    pub path: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::task) struct BoundPr {
    pub repository: String,
    pub repository_id: u64,
    pub installation_id: u64,
    pub number: u32,
    pub branch: String,
    pub base: String,
    pub head: String,
    pub tree: String,
    pub url: String,
    pub workflows: Vec<Workflow>,
}

#[derive(Deserialize)]
pub(super) struct Object { pub sha: String }

#[derive(Deserialize)]
pub(super) struct Commit {
    pub sha: String,
    pub tree: Object,
    pub parents: Vec<Object>,
    pub message: String,
}
