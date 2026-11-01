//! A deterministic compliant walk; it proves transport and recovery, not model judgment.
use crate::definition_repair::{NEW_COMMIT, Repo, github_auth, post, reconcile, write};
use crate::fake_model::{Asked, http, repair_branch, say};
use crate::repair_remote::draft_url;
use afr_providers::Chunk;
use serde_json::{Value, json};

pub(crate) const BLOCKED: &str = "Validation blocked: required remote state could not be verified.";

pub(crate) fn script(repo: Repo, reads: Vec<Value>) -> impl Fn(&Asked) -> Vec<Chunk> + Send + Sync {
    move |asked| {
        let branch = repair_branch(asked).unwrap_or_default();
        let base = 2 + reads.len();
        let failed = failed(asked);
        match asked.turn {
            0 => vec![read_reference(repo, branch, asked.turn)],
            1 if failed
                && !asked
                    .results
                    .last()
                    .is_some_and(|result| result.contains("Status: 404")) =>
            {
                vec![say(BLOCKED)]
            }
            1 => vec![reconcile(repo, branch, "reconcile-1")],
            turn if turn < base && failed => vec![say(BLOCKED)],
            turn if turn < base => reads.get(turn - 2).map_or_else(Vec::new, |read| {
                vec![http(&format!("required-read-{turn}"), read.clone())]
            }),
            _ => repair(repo, branch, asked, base),
        }
    }
}

fn repair(repo: Repo, branch: &str, asked: &Asked, base: usize) -> Vec<Chunk> {
    let stage = asked.turn - base;
    match stage {
        0..=3 | 5 | 8 if failed(asked) => vec![say(BLOCKED)],
        0..=2 => write(repo, branch, stage + 3),
        3 => vec![post(
            repo,
            "ref",
            "git/refs",
            &json!({"ref": format!("refs/heads/{branch}"), "sha": NEW_COMMIT}),
        )],
        4 | 7 => vec![read_reference(repo, branch, asked.turn)],
        5 | 8 => vec![reconcile(
            repo,
            branch,
            &format!("reconcile-{}", asked.turn),
        )],
        6 => after_ref(repo, branch, &asked.results),
        _ => vec![say(&if verified(&asked.results, branch, repo) {
            draft_url(repo)
        } else {
            BLOCKED.to_owned()
        })],
    }
}

fn failed(asked: &Asked) -> bool {
    asked
        .results
        .last()
        .is_some_and(|result| result.starts_with('['))
}

fn read_reference(repo: Repo, branch: &str, turn: usize) -> Chunk {
    http(
        &format!("read-ref-{turn}"),
        json!({"headers": github_auth(), "url": repo.url(&format!("git/ref/heads/{branch}"))}),
    )
}

fn after_ref(repo: Repo, branch: &str, results: &[String]) -> Vec<Chunk> {
    if verified(results, branch, repo) {
        return vec![say(&draft_url(repo))];
    }
    let ref_read = results.iter().rev().nth(1).and_then(|text| body(text));
    let pulls = results.last().and_then(|text| body(text));
    if ref_read
        .as_ref()
        .and_then(|value| value.pointer("/object/sha"))
        .and_then(Value::as_str)
        != Some(NEW_COMMIT)
        || pulls
            .as_ref()
            .and_then(Value::as_array)
            .is_none_or(|items| !items.is_empty())
    {
        return vec![say(BLOCKED)];
    }
    vec![post(
        repo,
        "draft",
        "pulls",
        &json!({"head": branch, "base": repo.base, "draft": true,
        "title": "fix: retry the fetch", "body": "Required evidence reviewed; validation must be observed."}),
    )]
}

pub(crate) fn verified(results: &[String], branch: &str, repo: Repo) -> bool {
    let reference = results.iter().rev().nth(1).and_then(|text| body(text));
    let pulls = results.last().and_then(|text| body(text));
    reference
        .as_ref()
        .and_then(|value| value.pointer("/object/sha"))
        .and_then(Value::as_str)
        == Some(NEW_COMMIT)
        && pulls
            .as_ref()
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items.len() == 1
                    && items.first().is_some_and(|draft| {
                        draft["html_url"] == draft_url(repo)
                            && draft.pointer("/head/ref").and_then(Value::as_str) == Some(branch)
                            && draft.pointer("/head/sha").and_then(Value::as_str)
                                == Some(NEW_COMMIT)
                            && draft.pointer("/base/ref").and_then(Value::as_str) == Some(repo.base)
                            && draft["draft"] == true
                    })
            })
}

fn body(text: &str) -> Option<Value> {
    if text.starts_with('[') {
        return None;
    }
    serde_json::from_str(text.split_once("\nResponse Body:\n")?.1).ok()
}
