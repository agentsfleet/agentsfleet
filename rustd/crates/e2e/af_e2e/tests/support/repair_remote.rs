//! GitHub effects commit independently of whether their HTTP replies arrive.
#![expect(
    clippy::expect_used,
    reason = "test support: invalid fixture state must fail loudly"
)]

use crate::definition_repair::{GITHUB, NEW_COMMIT, Repo, file_at_head, routes};
use crate::https::{Reply, Route, Seen};
use hyper::Method;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[path = "repair_updates.rs"]
mod updates;
#[path = "repair_validation.rs"]
mod validation;
pub(crate) use updates::{NEXT_BLOB, NEXT_COMMIT, NEXT_TREE};
pub(crate) use validation::{
    CHECK_RUN, CORRECTED_CHECK, CORRECTED_JOB, CORRECTED_RUN, VALIDATION_JOB, VALIDATION_RUN,
};

pub(crate) fn draft_url(repo: Repo) -> String {
    format!("https://github.com/{}/pull/9", repo.name)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Acknowledgement {
    Delivered,
    LoseRef,
    LoseDraft,
    LoseUpdate,
    LoseBeforeUpdate,
    RejectUpdate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Readback {
    Exact,
    Forbidden,
    WrongHead,
    ValidationForbidden,
    StaleValidation,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct State {
    pub(crate) reference: Option<String>,
    pub(crate) draft: Option<Value>,
    pub(crate) ref_posts: usize,
    pub(crate) draft_posts: usize,
    pub(crate) lost_replies: usize,
    pub(crate) ref_updates: usize,
    pub(crate) commit_parents: Vec<String>,
    pub(crate) blob_contents: Vec<String>,
    pub(crate) trees: Vec<Value>,
}

#[derive(Clone, Debug)]
pub(crate) struct Remote {
    repo: Repo,
    branch: String,
    acknowledgement: Acknowledgement,
    readback: Readback,
    state: Arc<Mutex<State>>,
}

impl Remote {
    pub(crate) fn new(
        repo: Repo,
        branch: &str,
        acknowledgement: Acknowledgement,
        readback: Readback,
    ) -> Self {
        Self {
            repo,
            branch: branch.to_owned(),
            acknowledgement,
            readback,
            state: Arc::default(),
        }
    }

    pub(crate) fn state(&self) -> State {
        self.state.lock().expect("the remote lock").clone()
    }

    pub(crate) fn routes(&self) -> Vec<Route> {
        let reference = self.repo.path(&format!("git/ref/heads/{}", self.branch));
        let refs = self.repo.path("git/refs");
        let pulls = self.repo.path("pulls");
        let mut result = routes(self.repo, None, file_at_head());
        let objects = ["git/blobs", "git/trees", "git/commits"].map(|path| self.repo.path(path));
        result.retain(|route| {
            route.path != pulls
                && route.path != refs
                && route.path != reference
                && !objects.contains(&route.path)
        });
        for (method, path) in [
            (Method::GET, reference),
            (Method::POST, refs),
            (Method::GET, pulls.clone()),
            (Method::POST, pulls),
        ] {
            let remote = self.clone();
            result.push(Route::stateful(
                GITHUB,
                method,
                &path,
                Arc::new(move |request| remote.answer(request)),
            ));
        }
        result.extend(self.continuation_routes());
        result
    }

    fn answer(&self, request: &Seen) -> Reply {
        let mut state = self.state.lock().expect("the remote lock");
        if request.method == Method::GET {
            return self.read(&state, request);
        }
        let body: Value = serde_json::from_str(&request.body).expect("a GitHub mutation is JSON");
        if request.method == Method::PATCH {
            return self.update(&mut state, &body);
        }
        if ["git/blobs", "git/trees", "git/commits"]
            .iter()
            .any(|path| request.path == self.repo.path(path))
        {
            return self.object(&mut state, request, &body);
        }
        if request.path == self.repo.path("git/refs") {
            state.ref_posts += 1;
            assert_eq!(
                body,
                json!({"ref": format!("refs/heads/{}", self.branch), "sha": NEW_COMMIT})
            );
            if state.reference.is_some() {
                return Reply::json(422, &json!({"message": "Reference already exists"}));
            }
            state.reference = Some(NEW_COMMIT.to_owned());
            return self.acknowledge(
                &mut state,
                Acknowledgement::LoseRef,
                &self.reference(NEW_COMMIT),
            );
        }
        state.draft_posts += 1;
        assert_eq!(
            (
                body.get("head").and_then(Value::as_str),
                body.get("base").and_then(Value::as_str),
                body.get("draft").and_then(Value::as_bool)
            ),
            (Some(self.branch.as_str()), Some(self.repo.base), Some(true))
        );
        assert_eq!(state.reference.as_deref(), Some(NEW_COMMIT));
        if state.draft.is_some() {
            return Reply::json(422, &json!({"message": "Pull request already exists"}));
        }
        let draft = json!({"html_url": draft_url(self.repo), "number": 9, "draft": true, "state": "open",
            "head": {"ref": self.branch, "sha": NEW_COMMIT}, "base": {"ref": self.repo.base}});
        state.draft = Some(draft.clone());
        self.acknowledge(&mut state, Acknowledgement::LoseDraft, &draft)
    }

    fn acknowledge(&self, state: &mut State, boundary: Acknowledgement, body: &Value) -> Reply {
        if self.acknowledgement == boundary && state.lost_replies == 0 {
            state.lost_replies += 1;
            return Reply::cut("{\"committed\":");
        }
        Reply::json(201, body)
    }

    fn read(&self, state: &State, request: &Seen) -> Reply {
        if state.lost_replies > 0 && self.readback == Readback::Forbidden {
            return Reply::json(403, &json!({"message": "Reconciliation unavailable"}));
        }
        if request.path.ends_with("/check-runs") || request.path.contains("/actions/") {
            return self.validation(state, request);
        }
        if request.path == self.repo.path("pulls") {
            let query: Vec<_> = url::form_urlencoded::parse(request.query.as_bytes())
                .into_owned()
                .collect();
            assert_eq!(query.len(), 3);
            let fields: std::collections::BTreeMap<_, _> = query.into_iter().collect();
            assert_eq!(fields.get("state").map(String::as_str), Some("all"));
            assert_eq!(fields.get("base").map(String::as_str), Some(self.repo.base));
            assert_eq!(
                fields.get("head"),
                Some(&format!(
                    "{}:{}",
                    self.repo.name.split('/').next().expect("owner"),
                    self.branch
                ))
            );
            return Reply::json(200, &json!(state.draft.iter().collect::<Vec<_>>()));
        }
        let Some(sha) = state.reference.as_deref() else {
            return Reply::json(404, &json!({"message": "Not Found"}));
        };
        let sha = if state.lost_replies > 0 && self.readback == Readback::WrongHead {
            crate::definition_repair::HEAD_SHA
        } else {
            sha
        };
        Reply::json(200, &self.reference(sha))
    }

    fn reference(&self, sha: &str) -> Value {
        json!({"ref": format!("refs/heads/{}", self.branch), "object": {"sha": sha, "type": "commit"}})
    }
}
