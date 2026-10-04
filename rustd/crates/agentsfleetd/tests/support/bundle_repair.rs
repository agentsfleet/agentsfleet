//! The repair walk both repairer bundles take, as a model and a GitHub.
//!
//! `ci-repairer/SKILL.md` and `incident-repairer/SKILL.md` share one shape:
//! reconcile the exact draft and ref, read the head and the files, then the
//! five Git Data writes and one draft Pull Request on the daemon-named branch.
//! [`script`] is that walk, deciding each turn from what came back — an
//! existing draft ends the run with its link, a failed read ends it
//! diagnosis-only — and [`routes`] is the GitHub it walks against.

use afr_providers::Chunk;
use hyper::Method;
use serde_json::{Value, json};

use crate::fake_model::{Asked, http, repair_branch, say};
use crate::https::{Reply, Route};

/// GitHub's API host.
pub(crate) const GITHUB: &str = "api.github.com";
/// The draft a run opens, and the one a second run finds.
pub(crate) const DRAFT_URL: &str = "https://github.com/agentsfleet/fixture/pull/9";
/// The head every read reports and every write builds on.
pub(crate) const HEAD_SHA: &str = "d3adb33fd3adb33fd3adb33fd3adb33fd3adb33f";
/// The file each repair corrects.
pub(crate) const FILE: &str = "src/lib/fetch.ts";
/// What a run that stopped after a failed read answers.
pub(crate) const DIAGNOSIS_ONLY: &str = "Diagnosis only: a read failed, so nothing was pushed.";
/// The head commit's tree, and what the three writes answer.
const BASE_TREE: &str = "7ee0000000000000000000000000000000000001";
const NEW_BLOB: &str = "b10b000000000000000000000000000000000002";
const NEW_TREE: &str = "7ee0000000000000000000000000000000000003";
const NEW_COMMIT: &str = "c0de000000000000000000000000000000000004";

/// The repository a repairer is bound to, and its trusted base.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Repo {
    pub(crate) name: &'static str,
    pub(crate) base: &'static str,
}

impl Repo {
    /// The API path of `rest` under this repository.
    pub(crate) fn path(self, rest: &str) -> String {
        format!("/repos/{}/{rest}", self.name)
    }

    fn url(self, rest: &str) -> String {
        format!("https://{GITHUB}{}", self.path(rest))
    }
}

/// The `Authorization` header every GitHub call writes.
pub(crate) fn github_auth() -> Value {
    json!({"Authorization": "Bearer ${secrets.github.token}"})
}

/// The walk, with `reads` — the bundle's evidence reads plus the head and the
/// file — made in one turn after reconciliation, and `refused` — requests the
/// policy must turn away — tried beside the ref read, where their refusals
/// never count as a failed read.
pub(crate) fn script(
    repo: Repo,
    reads: Vec<Value>,
    refused: Vec<Value>,
) -> impl Fn(&Asked) -> Vec<Chunk> + Send + Sync {
    move |asked| {
        let branch = repair_branch(asked).unwrap_or_default();
        let owner = repo.name.split('/').next().unwrap_or_default();
        let post = |id: &str, rest: &str, body: Value| {
            http(
                id,
                json!({"url": repo.url(rest), "method": "POST",
                            "headers": github_auth(), "body": body.to_string()}),
            )
        };
        match asked.turn {
            0 => vec![http(
                "reconcile",
                json!({"headers": github_auth(), "url": repo.url(
                &format!("pulls?state=all&head={owner}:{branch}&base={}", repo.base))}),
            )],
            1 if asked
                .results
                .iter()
                .any(|result| result.contains(DRAFT_URL)) =>
            {
                vec![say(&format!("The draft is already open: {DRAFT_URL}"))]
            }
            1 => std::iter::once(http(
                "ref",
                json!({"headers": github_auth(),
                                         "url": repo.url(&format!("git/ref/heads/{branch}"))}),
            ))
            .chain(
                (refused.iter().enumerate())
                    .map(|(index, probe)| http(&format!("refused-{index}"), probe.clone())),
            )
            .collect(),
            2 => (reads.iter().enumerate())
                .map(|(index, read)| http(&format!("read-{index}"), read.clone()))
                .collect(),
            3 if asked
                .results
                .iter()
                .rev()
                .take(reads.len())
                .any(|result| result.starts_with('[')) =>
            {
                vec![say(DIAGNOSIS_ONLY)]
            }
            3 => vec![post(
                "blob",
                "git/blobs",
                json!({"content": "export const retries = 3;\n", "encoding": "utf-8"}),
            )],
            4 => vec![post(
                "tree",
                "git/trees",
                json!({"base_tree": BASE_TREE,
                "tree": [{"path": FILE, "mode": "100644", "type": "blob", "sha": NEW_BLOB}]}),
            )],
            5 => vec![post(
                "commit",
                "git/commits",
                json!({"message": "fix: retry the fetch",
                "tree": NEW_TREE, "parents": [HEAD_SHA]}),
            )],
            // The base's own ref first: the lock the daemon compiled refuses it
            // before it leaves, and the run goes on to the one it names.
            6 => vec![
                post(
                    "ref-base",
                    "git/refs",
                    json!({"ref": format!("refs/heads/{}", repo.base), "sha": NEW_COMMIT}),
                ),
                post(
                    "ref",
                    "git/refs",
                    json!({"ref": format!("refs/heads/{branch}"), "sha": NEW_COMMIT}),
                ),
            ],
            7 => vec![post(
                "draft",
                "pulls",
                json!({"title": "fix: retry the fetch",
                "head": branch, "base": repo.base, "draft": true, "body": "Cause and evidence."}),
            )],
            _ => vec![say(&format!("Opened {DRAFT_URL} against {}.", repo.base))],
        }
    }
}

/// The head and file reads every walk makes, `contents` answering the file.
pub(crate) fn head_reads(repo: Repo) -> Vec<Value> {
    vec![
        json!({"headers": github_auth(), "url": repo.url(&format!("branches/{}", repo.base))}),
        json!({"headers": github_auth(), "url": repo.url(&format!("contents/{FILE}?ref={HEAD_SHA}"))}),
    ]
}

/// GitHub for one run: `open_draft`, when set, is the branch an earlier run
/// already opened a draft from, and `contents` answers the file read.
pub(crate) fn routes(repo: Repo, open_draft: Option<&str>, contents: Reply) -> Vec<Route> {
    let pulls = open_draft.map_or_else(Vec::new, |branch| {
        vec![
            json!({"html_url": DRAFT_URL, "draft": true, "state": "open",
                    "head": {"ref": branch}, "base": {"ref": repo.base}}),
        ]
    });
    let created = |sha: &str| Reply::json(201, &json!({"sha": sha}));
    vec![
        Route::new(
            GITHUB,
            Method::GET,
            &repo.path("pulls"),
            vec![Reply::json(200, &json!(pulls))],
        ),
        Route::new(
            GITHUB,
            Method::GET,
            &repo.path(&format!("branches/{}", repo.base)),
            vec![Reply::json(
                200,
                &json!({"name": repo.base,
                "commit": {"sha": HEAD_SHA, "commit": {"tree": {"sha": BASE_TREE}}}}),
            )],
        ),
        Route::new(
            GITHUB,
            Method::GET,
            &repo.path(&format!("contents/{FILE}")),
            vec![contents],
        ),
        Route::new(
            GITHUB,
            Method::POST,
            &repo.path("git/blobs"),
            vec![created(NEW_BLOB)],
        ),
        Route::new(
            GITHUB,
            Method::POST,
            &repo.path("git/trees"),
            vec![created(NEW_TREE)],
        ),
        Route::new(
            GITHUB,
            Method::POST,
            &repo.path("git/commits"),
            vec![created(NEW_COMMIT)],
        ),
        Route::new(
            GITHUB,
            Method::POST,
            &repo.path("git/refs"),
            vec![created(NEW_COMMIT)],
        ),
        Route::new(
            GITHUB,
            Method::POST,
            &repo.path("pulls"),
            vec![Reply::json(
                201,
                &json!({"html_url": DRAFT_URL, "number": 9, "draft": true}),
            )],
        ),
    ]
}

/// The file as GitHub answers it at the head.
pub(crate) fn file_at_head() -> Reply {
    Reply::json(
        200,
        &json!({"path": FILE, "sha": HEAD_SHA, "encoding": "base64",
                             "content": "ZXhwb3J0IGNvbnN0IHJldHJpZXMgPSAwOwo="}),
    )
}
