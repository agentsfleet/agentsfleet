//! Committed branch updates and validation responses behind the local GitHub boundary.
use super::{Acknowledgement, Remote, State};
use crate::definition_repair::{HEAD_SHA, NEW_BLOB, NEW_COMMIT, NEW_TREE};
use crate::https::{Reply, Route, Seen};
use hyper::Method;
use serde_json::{Value, json};
use std::sync::Arc;

pub(crate) const NEXT_BLOB: &str = "b10b000000000000000000000000000000000005";
pub(crate) const NEXT_TREE: &str = "7ee0000000000000000000000000000000000006";
pub(crate) const NEXT_COMMIT: &str = "c0de000000000000000000000000000000000007";

impl Remote {
    pub(super) fn continuation_routes(&self) -> Vec<Route> {
        let paths = [
            (Method::PATCH, format!("git/refs/heads/{}", self.branch)),
            (Method::POST, "git/blobs".to_owned()),
            (Method::POST, "git/trees".to_owned()),
            (Method::POST, "git/commits".to_owned()),
            (Method::GET, format!("commits/{NEW_COMMIT}/check-runs")),
            (Method::GET, format!("commits/{NEXT_COMMIT}/check-runs")),
            (
                Method::GET,
                format!("actions/runs/{}", super::VALIDATION_RUN),
            ),
            (
                Method::GET,
                format!("actions/runs/{}/attempts/1/jobs", super::VALIDATION_RUN),
            ),
            (
                Method::GET,
                format!("actions/jobs/{}", super::VALIDATION_JOB),
            ),
            (
                Method::GET,
                format!("actions/jobs/{}/logs", super::VALIDATION_JOB),
            ),
        ];
        let corrected = [
            format!("actions/runs/{}", super::CORRECTED_RUN),
            format!("actions/runs/{}/attempts/1/jobs", super::CORRECTED_RUN),
            format!("actions/jobs/{}", super::CORRECTED_JOB),
        ];
        paths
            .into_iter()
            .chain(corrected.into_iter().map(|path| (Method::GET, path)))
            .map(|(method, path)| {
                let remote = self.clone();
                Route::stateful(
                    crate::definition_repair::GITHUB,
                    method,
                    &self.repo.path(&path),
                    Arc::new(move |request| remote.answer(request)),
                )
            })
            .collect()
    }

    pub(super) fn object(&self, state: &mut State, request: &Seen, body: &Value) -> Reply {
        if request.path == self.repo.path("git/blobs") {
            state
                .blob_contents
                .push(body["content"].as_str().expect("blob content").to_owned());
            return Reply::json(
                201,
                &json!({"sha": if state.blob_contents.len() == 1 {NEW_BLOB} else {NEXT_BLOB}}),
            );
        }
        if request.path == self.repo.path("git/trees") {
            state.trees.push(body.clone());
            return Reply::json(
                201,
                &json!({"sha": if state.trees.len() == 1 {NEW_TREE} else {NEXT_TREE}}),
            );
        }
        let parent = state.reference.as_deref().unwrap_or(HEAD_SHA);
        assert_eq!(
            body["parents"],
            json!([parent]),
            "every candidate builds on the observed current head"
        );
        state.commit_parents.push(parent.to_owned());
        let (sha, tree) = if state.commit_parents.len() == 1 {
            (NEW_COMMIT, NEW_TREE)
        } else {
            (NEXT_COMMIT, NEXT_TREE)
        };
        assert_eq!(body["tree"], tree);
        Reply::json(
            201,
            &json!({"sha": sha, "tree": {"sha": tree}, "parents": [{"sha": parent}]}),
        )
    }

    pub(super) fn update(&self, state: &mut State, body: &Value) -> Reply {
        state.ref_updates += 1;
        assert_eq!(*body, json!({"sha": NEXT_COMMIT, "force": false}));
        if self.acknowledgement == Acknowledgement::RejectUpdate {
            return Reply::json(422, &json!({"message": "Update is not a fast forward"}));
        }
        if self.acknowledgement == Acknowledgement::LoseBeforeUpdate && state.lost_replies == 0 {
            state.lost_replies += 1;
            return Reply::cut("{\"not_committed\":");
        }
        assert_eq!(state.reference.as_deref(), Some(NEW_COMMIT));
        state.reference = Some(NEXT_COMMIT.to_owned());
        let draft = state.draft.as_mut().expect("the existing draft");
        *draft
            .pointer_mut("/head/sha")
            .expect("the draft has a head") = json!(NEXT_COMMIT);
        if self.acknowledgement == Acknowledgement::LoseUpdate && state.lost_replies == 0 {
            state.lost_replies += 1;
            return Reply::cut("{\"committed\":");
        }
        Reply::json(200, &self.reference(NEXT_COMMIT))
    }
}
