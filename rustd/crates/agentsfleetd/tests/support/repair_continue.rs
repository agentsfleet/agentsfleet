//! A failed validation updates the same event's draft, with bounded reconciliation.
use crate::definition_repair::{FILE, NEW_COMMIT, NEW_TREE, Repo, github_auth, post, reconcile};
use crate::fake_model::{Asked, http, repair_branch, say};
use crate::repair_remote::{
    CHECK_RUN, CORRECTED_CHECK, CORRECTED_JOB, CORRECTED_RUN, NEXT_BLOB, NEXT_COMMIT, NEXT_TREE,
    VALIDATION_JOB, VALIDATION_RUN, draft_url,
};
use crate::repair_walk::{self, BLOCKED};
use afr_providers::Chunk;
use serde_json::{Value, json};

pub(crate) fn script(repo: Repo, reads: Vec<Value>) -> impl Fn(&Asked) -> Vec<Chunk> + Send + Sync {
    let correction = 2 + reads.len() + 9;
    let initial = repair_walk::script(repo, reads);
    move |asked| {
        if asked.turn < correction {
            return initial(asked);
        }
        let branch = repair_branch(asked).unwrap_or_default();
        let stage = asked.turn - correction;
        if stage == 0 && !repair_walk::verified(&asked.results, branch, repo) {
            return vec![say(BLOCKED)];
        }
        correct(repo, branch, asked, stage)
    }
}

fn correct(repo: Repo, branch: &str, asked: &Asked, stage: usize) -> Vec<Chunk> {
    match stage {
        0..=4 => failed_validation(repo, asked, stage),
        5..=10 if refused(asked) => vec![say(BLOCKED)],
        5 => vec![reference(repo, branch, "fresh-own-ref")],
        6 => vec![reconcile(repo, branch, "fresh-own-draft")],
        7 if !repair_walk::verified(&asked.results, branch, repo) => vec![say(BLOCKED)],
        7..=10 => candidate(repo, branch, stage - 3),
        11 => vec![reference(repo, branch, "update-ref")],
        12 if !refused(asked) => vec![reconcile(repo, branch, "update-draft")],
        13 => after_update(repo, branch, asked),
        14 => after_verification_or_retry(repo, branch, asked),
        15..=20 => finish_verification(repo, branch, asked),
        _ => vec![say(BLOCKED)],
    }
}

fn failed_validation(repo: Repo, asked: &Asked, stage: usize) -> Vec<Chunk> {
    match stage {
        0 => vec![get(
            repo,
            "initial-checks",
            &format!("commits/{NEW_COMMIT}/check-runs"),
        )],
        1 if checks(asked, repo, NEW_COMMIT, "failure") => vec![get(
            repo,
            "validation-run",
            &format!("actions/runs/{VALIDATION_RUN}"),
        )],
        2 if run_matches(asked) => vec![get(
            repo,
            "validation-jobs",
            &format!("actions/runs/{VALIDATION_RUN}/attempts/1/jobs?per_page=100&page=1"),
        )],
        3 if job_list_matches(asked) => vec![get(
            repo,
            "validation-job",
            &format!("actions/jobs/{VALIDATION_JOB}"),
        )],
        4 if job_matches(asked.results.last().and_then(|text| body(text)).as_ref()) => vec![get(
            repo,
            "failed-log",
            &format!("actions/jobs/{VALIDATION_JOB}/logs"),
        )],
        _ => vec![say(BLOCKED)],
    }
}

fn after_update(repo: Repo, branch: &str, asked: &Asked) -> Vec<Chunk> {
    if settled(&asked.results, NEXT_COMMIT, branch, repo) {
        return vec![get(
            repo,
            "corrected-checks",
            &format!("commits/{NEXT_COMMIT}/check-runs"),
        )];
    }
    if settled(&asked.results, NEW_COMMIT, branch, repo)
        && asked
            .results
            .iter()
            .rev()
            .nth(2)
            .is_some_and(|result| result.starts_with("[upstream_unreachable]"))
    {
        return vec![update(repo, branch)];
    }
    vec![say(BLOCKED)]
}

fn settled(results: &[String], expected: &str, branch: &str, repo: Repo) -> bool {
    let reference = results.iter().rev().nth(1).and_then(|text| body(text));
    let pulls = results.last().and_then(|text| body(text));
    reference
        .as_ref()
        .and_then(|value| value.pointer("/object/sha"))
        .and_then(Value::as_str)
        == Some(expected)
        && pulls
            .as_ref()
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items.len() == 1
                    && items.first().is_some_and(|draft| {
                        draft["html_url"] == draft_url(repo)
                            && draft["draft"] == true
                            && draft["state"] == "open"
                            && draft.pointer("/head/ref").and_then(Value::as_str) == Some(branch)
                            && draft.pointer("/head/sha").and_then(Value::as_str) == Some(expected)
                            && draft.pointer("/base/ref").and_then(Value::as_str) == Some(repo.base)
                    })
            })
}

fn checks(asked: &Asked, repo: Repo, head: &str, conclusion: &str) -> bool {
    asked
        .results
        .last()
        .and_then(|text| body(text))
        .is_some_and(|value| {
            value.get("total_count").and_then(Value::as_u64) == Some(1)
                && value
                    .get("check_runs")
                    .and_then(Value::as_array)
                    .is_some_and(|items| {
                        items.len() == 1
                            && items.first().is_some_and(|check| {
                                check["id"]
                                    == if head == NEXT_COMMIT {
                                        CORRECTED_CHECK
                                    } else {
                                        CHECK_RUN
                                    }
                                    && check["details_url"]
                                        == format!(
                                            "https://github.com/{}/actions/runs/{}",
                                            repo.name,
                                            if head == NEXT_COMMIT {
                                                CORRECTED_RUN
                                            } else {
                                                VALIDATION_RUN
                                            }
                                        )
                                    && check["head_sha"] == head
                                    && check["name"] == "title-regression"
                                    && check["status"] == "completed"
                                    && check["conclusion"] == conclusion
                            })
                    })
        })
}

fn get(repo: Repo, id: &str, path: &str) -> Chunk {
    http(id, json!({"headers": github_auth(), "url": repo.url(path)}))
}
fn reference(repo: Repo, branch: &str, id: &str) -> Chunk {
    get(repo, id, &format!("git/ref/heads/{branch}"))
}
fn update(repo: Repo, branch: &str) -> Chunk {
    http(
        "update-own-ref",
        json!({"headers": github_auth(),
    "url": repo.url(&format!("git/refs/heads/{branch}")), "method": "PATCH", "body": json!({"sha": NEXT_COMMIT, "force": false}).to_string()}),
    )
}
fn refused(asked: &Asked) -> bool {
    asked
        .results
        .last()
        .is_some_and(|result| result.starts_with('['))
}
fn body(text: &str) -> Option<Value> {
    if text.starts_with('[') {
        None
    } else {
        serde_json::from_str(text.split_once("\nResponse Body:\n")?.1).ok()
    }
}

fn candidate(repo: Repo, branch: &str, stage: usize) -> Vec<Chunk> {
    match stage {
        4 => vec![post(
            repo,
            "corrected-blob",
            "git/blobs",
            &json!({"content": "export const retries = 4;\n", "encoding": "utf-8"}),
        )],
        5 => vec![post(
            repo,
            "corrected-tree",
            "git/trees",
            &json!({"base_tree": NEW_TREE,
            "tree": [{"path": FILE, "mode": "100644", "type": "blob", "sha": NEXT_BLOB}]}),
        )],
        6 => vec![post(
            repo,
            "corrected-commit",
            "git/commits",
            &json!({"message": "fix: correct the retry count", "tree": NEXT_TREE, "parents": [NEW_COMMIT]}),
        )],
        _ => vec![update(repo, branch)],
    }
}

fn after_verification_or_retry(repo: Repo, branch: &str, asked: &Asked) -> Vec<Chunk> {
    let Some((latest, prior)) = asked.results.split_last() else {
        return vec![say(BLOCKED)];
    };
    if settled(prior, NEXT_COMMIT, branch, repo) {
        return if checks(asked, repo, NEXT_COMMIT, "success") {
            vec![get(
                repo,
                "corrected-run",
                &format!("actions/runs/{CORRECTED_RUN}"),
            )]
        } else {
            vec![say(BLOCKED)]
        };
    }
    let uncertain = latest.starts_with("[upstream_unreachable]");
    let acknowledged = body(latest).is_some_and(|value| {
        value.pointer("/object/sha").and_then(Value::as_str) == Some(NEXT_COMMIT)
    });
    if settled(prior, NEW_COMMIT, branch, repo) && (uncertain || acknowledged) {
        return vec![reference(repo, branch, "retry-update-ref")];
    }
    vec![say(BLOCKED)]
}

fn run_matches(asked: &Asked) -> bool {
    asked
        .results
        .last()
        .and_then(|text| body(text))
        .is_some_and(|run| {
            run.get("id").and_then(Value::as_u64) == Some(VALIDATION_RUN)
                && run.get("run_attempt").and_then(Value::as_u64) == Some(1)
                && run.get("head_sha").and_then(Value::as_str) == Some(NEW_COMMIT)
        })
}

fn job_list_matches(asked: &Asked) -> bool {
    asked
        .results
        .last()
        .and_then(|text| body(text))
        .is_some_and(|value| {
            value.get("total_count").and_then(Value::as_u64) == Some(1)
                && value
                    .get("jobs")
                    .and_then(Value::as_array)
                    .is_some_and(|jobs| jobs.len() == 1 && job_matches(jobs.first()))
        })
}

fn job_matches(job: Option<&Value>) -> bool {
    job.is_some_and(|job| {
        job["id"] == VALIDATION_JOB
            && job["run_id"] == VALIDATION_RUN
            && job["run_attempt"] == 1
            && job["head_sha"] == NEW_COMMIT
            && job["conclusion"] == "failure"
            && job["check_run_url"]
                .as_str()
                .is_some_and(|url| url.ends_with(&format!("/check-runs/{CHECK_RUN}")))
    })
}

fn finish_verification(repo: Repo, branch: &str, asked: &Asked) -> Vec<Chunk> {
    let Some(value) = asked.results.last().and_then(|text| body(text)) else {
        return vec![say(BLOCKED)];
    };
    if value.pointer("/object/sha").and_then(Value::as_str) == Some(NEXT_COMMIT) {
        return vec![reconcile(repo, branch, "retry-update-draft")];
    }
    if settled(&asked.results, NEXT_COMMIT, branch, repo) {
        return vec![get(
            repo,
            "corrected-checks",
            &format!("commits/{NEXT_COMMIT}/check-runs"),
        )];
    }
    if checks(asked, repo, NEXT_COMMIT, "success") {
        return vec![get(
            repo,
            "corrected-run",
            &format!("actions/runs/{CORRECTED_RUN}"),
        )];
    }
    finish_jobs(repo, &value)
}

fn finish_jobs(repo: Repo, value: &Value) -> Vec<Chunk> {
    if value["id"] == CORRECTED_RUN && value["run_attempt"] == 1 && value["head_sha"] == NEXT_COMMIT
    {
        return vec![get(
            repo,
            "corrected-jobs",
            &format!("actions/runs/{CORRECTED_RUN}/attempts/1/jobs?per_page=100&page=1"),
        )];
    }
    if value.get("total_count").and_then(Value::as_u64) == Some(1)
        && value["jobs"]
            .as_array()
            .is_some_and(|jobs| jobs.len() == 1 && corrected_job(jobs.first(), repo))
    {
        return vec![get(
            repo,
            "corrected-job",
            &format!("actions/jobs/{CORRECTED_JOB}"),
        )];
    }
    vec![say(&if corrected_job(Some(value), repo) {
        draft_url(repo)
    } else {
        BLOCKED.to_owned()
    })]
}

fn corrected_job(job: Option<&Value>, repo: Repo) -> bool {
    job.is_some_and(|job| {
        job["id"] == CORRECTED_JOB
            && job["run_id"] == CORRECTED_RUN
            && job["run_attempt"] == 1
            && job["head_sha"] == NEXT_COMMIT
            && job["conclusion"] == "success"
            && job["check_run_url"] == repo.url(&format!("check-runs/{CORRECTED_CHECK}"))
    })
}
