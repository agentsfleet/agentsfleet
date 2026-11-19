//! The tested tree is rebuilt from the expected head before any ref moves:
//! each change lands on a regular file, under directories, once, within one
//! MiB, and the rebuilt tree must be the tested one.

#![expect(
    clippy::expect_used,
    reason = "a fixture input the wire bounds refuse fails the test loudly"
)]

use af_wire::task::{DecisionTaskVersion, TaskEffectInput, TaskFileChange};
use serde_json::{Value, json};

use super::reproduce;
use crate::{
    Result,
    error::ErrorKind,
    task::{
        TaskRefusal,
        github::{
            BoundPr,
            double::{INSTALLATION, Origin, Reply},
        },
    },
};

const HEAD: &str = "1111111111111111111111111111111111111111";
const BASE: &str = "2222222222222222222222222222222222222222";
const SOURCE: &str = "7777777777777777777777777777777777777777";
const TESTED: &str = "6666666666666666666666666666666666666666";
const COMMIT: &str =
    "GET /repos/example/linkwarden/git/commits/1111111111111111111111111111111111111111";
const ROOT: &str =
    "GET /repos/example/linkwarden/git/trees/2222222222222222222222222222222222222222";
const SOURCE_TREE: &str =
    "GET /repos/example/linkwarden/git/trees/7777777777777777777777777777777777777777";
const BLOBS: &str = "POST /repos/example/linkwarden/git/blobs";
const TREES: &str = "POST /repos/example/linkwarden/git/trees";
/// `fix`, base64-encoded.
const FIX: &str = "Zml4";

fn target() -> BoundPr {
    BoundPr {
        repository: "example/linkwarden".to_owned(),
        repository_id: 42,
        installation_id: INSTALLATION,
        number: 12,
        branch: "repair".to_owned(),
        base: "main".to_owned(),
        head: HEAD.to_owned(),
        tree: BASE.to_owned(),
        url: "https://github.com/example/linkwarden/pull/12".to_owned(),
        workflows: Vec::new(),
    }
}

/// A change writing `contents` to `path`, or deleting it when `None`.
fn change(path: &str, contents: Option<&str>) -> TaskFileChange {
    TaskFileChange {
        path: path.to_owned(),
        contents: contents.map(str::to_owned),
    }
}

fn input(changes: Vec<TaskFileChange>) -> garde::Valid<TaskEffectInput> {
    garde::Unvalidated::new(TaskEffectInput {
        operation_id: "01924f4e-0000-7000-8000-0000000000c3".to_owned(),
        fencing_token: 1,
        task: DecisionTaskVersion {
            task_id: "01924f4e-0000-7000-8000-0000000000a1".to_owned(),
            generation: 1,
        },
        source_item_id: "01924f4e-0000-7000-8000-0000000000d4".to_owned(),
        covered_input_sequence: 1,
        expected_head: HEAD.to_owned(),
        tested_tree: TESTED.to_owned(),
        changes,
    })
    .validate()
    .expect("fixture effect within the wire bounds")
}

/// The expected head, whose tree is `BASE`.
fn commit() -> Reply {
    Reply::json(&json!({"sha": HEAD, "tree": {"sha": BASE}}))
}

/// One tree listing of `entries`, complete unless `truncated`.
fn tree(entries: &Value, truncated: bool) -> Reply {
    Reply::json(&json!({"tree": entries, "truncated": truncated}))
}

fn object(sha: &str) -> Reply {
    Reply::json(&json!({"sha": sha}))
}

/// The refusal a result carries, `Err(None)` for any failure that is not one.
fn outcome(result: Result<()>) -> core::result::Result<(), Option<TaskRefusal>> {
    result.map_err(|error| match error.kind() {
        ErrorKind::TaskRefused { reason } => Some(*reason),
        _ => None,
    })
}

/// `src/lib.rs` is rewritten beneath `src`, and `README.md` is deleted.
#[tokio::test]
async fn a_nested_write_and_a_deletion_rebuild_the_tested_tree() {
    let root = json!([
        {"path": "src", "mode": "040000", "sha": SOURCE},
        {"path": "README.md", "mode": "100644", "sha": "8888888888888888888888888888888888888888"},
    ]);
    let origin = Origin::start(vec![
        commit(),
        tree(&root, false),
        tree(
            &json!([{"path": "lib.rs", "mode": "100644", "sha": "9999999999999999999999999999999999999999"}]),
            false,
        ),
        object("5555555555555555555555555555555555555555"),
        tree(&root, false),
        object(TESTED),
    ])
    .await;
    let effect = input(vec![
        change("src/lib.rs", Some(FIX)),
        change("README.md", None),
    ]);

    let rebuilt = reproduce(&origin.session(false), &target(), &effect).await;

    assert_eq!(outcome(rebuilt), Ok(()));
    assert_eq!(
        origin.requests(),
        [COMMIT, ROOT, SOURCE_TREE, BLOBS, ROOT, TREES]
    );
}

#[tokio::test]
async fn a_head_that_is_not_the_expected_commit_is_an_invalid_target() {
    let origin = Origin::start(vec![Reply::json(&json!({
        "sha": "4444444444444444444444444444444444444444",
        "tree": {"sha": BASE},
    }))])
    .await;
    let effect = input(vec![change("README.md", Some(FIX))]);

    let rebuilt = reproduce(&origin.session(false), &target(), &effect).await;

    assert_eq!(outcome(rebuilt), Err(Some(TaskRefusal::InvalidTarget)));
    assert_eq!(origin.requests(), [COMMIT]);
}

/// `docs` is written first; `docs/a.md` beneath it is refused before any
/// request for it.
#[tokio::test]
async fn a_change_beneath_an_earlier_change_is_an_invalid_target() {
    let origin = Origin::start(vec![
        commit(),
        tree(&json!([]), false),
        object("5555555555555555555555555555555555555555"),
    ])
    .await;
    let effect = input(vec![
        change("docs", Some(FIX)),
        change("docs/a.md", Some(FIX)),
    ]);

    let rebuilt = reproduce(&origin.session(false), &target(), &effect).await;

    assert_eq!(outcome(rebuilt), Err(Some(TaskRefusal::InvalidTarget)));
    assert_eq!(origin.requests(), [COMMIT, ROOT, BLOBS]);
}

#[tokio::test]
async fn a_truncated_listing_a_missing_deletion_or_an_irregular_entry_is_an_invalid_target() {
    for (listing, written) in [
        (tree(&json!([]), true), change("README.md", Some(FIX))),
        (tree(&json!([]), false), change("README.md", None)),
        (
            tree(
                &json!([{"path": "src", "mode": "100644", "sha": SOURCE}]),
                false,
            ),
            change("src/lib.rs", Some(FIX)),
        ),
        (
            tree(
                &json!([{"path": "README.md", "mode": "120000", "sha": SOURCE}]),
                false,
            ),
            change("README.md", Some(FIX)),
        ),
        (tree(&json!([]), false), change("README.md", Some("%%%%"))),
    ] {
        let path = written.path.clone();
        let origin = Origin::start(vec![commit(), listing]).await;
        let effect = input(vec![written]);

        let rebuilt = reproduce(&origin.session(false), &target(), &effect).await;

        assert_eq!(
            outcome(rebuilt),
            Err(Some(TaskRefusal::InvalidTarget)),
            "{path}"
        );
        assert_eq!(origin.requests(), [COMMIT, ROOT], "{path}");
    }
}

/// The longest contents the wire admits decode to two bytes past one MiB.
#[tokio::test]
async fn contents_past_one_mebibyte_exhaust_the_limit() {
    let origin = Origin::start(vec![commit(), tree(&json!([]), false)]).await;
    let largest = "A".repeat(1_398_104);
    let effect = input(vec![change("README.md", Some(&largest))]);

    let rebuilt = reproduce(&origin.session(false), &target(), &effect).await;

    assert_eq!(outcome(rebuilt), Err(Some(TaskRefusal::LimitExhausted)));
    assert_eq!(origin.requests(), [COMMIT, ROOT]);
}

#[tokio::test]
async fn a_rebuilt_tree_that_is_not_the_tested_one_is_an_invalid_target() {
    for rebuilt_sha in [BASE, "3333333333333333333333333333333333333333"] {
        let origin = Origin::start(vec![
            commit(),
            tree(&json!([]), false),
            object("5555555555555555555555555555555555555555"),
            object(rebuilt_sha),
        ])
        .await;
        let effect = input(vec![change("README.md", Some(FIX))]);

        let rebuilt = reproduce(&origin.session(false), &target(), &effect).await;

        assert_eq!(
            outcome(rebuilt),
            Err(Some(TaskRefusal::InvalidTarget)),
            "{rebuilt_sha}"
        );
        assert_eq!(origin.requests(), [COMMIT, ROOT, BLOBS, TREES]);
    }
}
