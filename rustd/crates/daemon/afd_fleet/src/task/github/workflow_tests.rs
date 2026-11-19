//! A reviewed workflow is read-only, names hosted runners and references no
//! secret; every privileged shape is refused as an invalid target.

use af_fleet_runtime::config::ValidationWorkflow;
use base64::{Engine as _, prelude::BASE64_STANDARD};
use serde_json::json;

use super::{Workflow, review, safe, verify};
use crate::{
    Result,
    error::ErrorKind,
    task::{
        TaskRefusal,
        github::double::{Origin, Reply},
    },
};

const HEAD: &str = "1111111111111111111111111111111111111111";
const LISTING: &str = "GET /repos/example/linkwarden/contents/.github/workflows?ref=1111111111111111111111111111111111111111";
const BLOB: &str =
    "GET /repos/example/linkwarden/git/blobs/5151515151515151515151515151515151515151";
const PATH: &str = ".github/workflows/ci.yml";

/// The refusal a result carries, `Err(None)` for any failure that is not one.
fn outcome(result: Result<()>) -> core::result::Result<(), Option<TaskRefusal>> {
    result.map_err(|error| match error.kind() {
        ErrorKind::TaskRefused { reason } => Some(*reason),
        _ => None,
    })
}

/// A permission map of `read` and `none`, at both levels, with a number among
/// the values the secret scan walks.
const READ_ONLY: &str = "on: push
permissions:
  contents: read
  actions: none
jobs:
  test:
    runs-on: ubuntu-24.04
    timeout-minutes: 5
    permissions:
      contents: read
    steps:
      - run: echo ok
";

#[test]
fn a_read_only_workflow_on_a_hosted_runner_is_safe() {
    assert_eq!(outcome(safe(READ_ONLY)), Ok(()));
}

#[test]
fn a_document_without_a_jobs_mapping_is_refused() {
    for text in [
        "on: push\njobs: [\n",
        "- push\n",
        "on: push\npermissions: read-all\njobs: []\n",
        "on: push\npermissions: read-all\njobs: {}\n",
    ] {
        assert_eq!(
            outcome(safe(text)),
            Err(Some(TaskRefusal::InvalidTarget)),
            "{text}"
        );
    }
}

#[test]
fn a_workflow_without_read_permissions_or_a_trigger_is_refused() {
    for text in [
        "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n",
        "on: push\npermissions:\n  contents: write\njobs:\n  test:\n    runs-on: ubuntu-latest\n",
        "on: push\npermissions: write-all\njobs:\n  test:\n    runs-on: ubuntu-latest\n",
        "permissions: read-all\njobs:\n  test:\n    runs-on: ubuntu-latest\n",
    ] {
        assert_eq!(
            outcome(safe(text)),
            Err(Some(TaskRefusal::InvalidTarget)),
            "{text}"
        );
    }
}

#[test]
fn a_workflow_naming_a_secret_is_refused() {
    let text = "on: push\npermissions: read-all\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: \"echo ${{ secrets.TOKEN }}\"\n";

    assert_eq!(outcome(safe(text)), Err(Some(TaskRefusal::InvalidTarget)));
}

#[test]
fn a_job_with_an_environment_a_reusable_call_a_private_runner_or_write_access_is_refused() {
    for job in [
        "    runs-on: ubuntu-latest\n    environment: production\n",
        "    runs-on: ubuntu-latest\n    uses: example/linkwarden/.github/workflows/ci.yml@main\n",
        "    runs-on: self-hosted\n",
        "    runs-on: [ubuntu-latest]\n",
        "    runs-on: ubuntu-latest\n    permissions:\n      contents: write\n",
    ] {
        let text = format!("on: push\npermissions: read-all\njobs:\n  test:\n{job}");
        assert_eq!(
            outcome(safe(&text)),
            Err(Some(TaskRefusal::InvalidTarget)),
            "{text}"
        );
    }
}

/// The reviewed `ci.yml`, digested as `digest`.
fn reviewed(digest: String) -> Vec<Workflow> {
    vec![Workflow {
        id: 100,
        path: PATH.to_owned(),
        digest,
    }]
}

/// The directory listing GitHub answers: one entry of `kind` at `PATH`.
fn listing(kind: &str) -> Reply {
    Reply::json(&json!([{
        "path": PATH,
        "sha": "5151515151515151515151515151515151515151",
        "type": kind,
    }]))
}

fn blob(content: &str, encoding: &str) -> Reply {
    Reply::json(&json!({"content": content, "encoding": encoding}))
}

/// GitHub wraps a blob's base64 across lines; the wrap is not content.
#[tokio::test]
async fn reviewed_content_at_the_head_verifies() {
    let encoded = BASE64_STANDARD.encode(READ_ONLY);
    let (first, rest) = encoded.split_at(60);
    let origin = Origin::start(vec![
        listing("file"),
        blob(&format!("{first}\n{rest}\n"), "base64"),
    ])
    .await;
    let workflows = reviewed(af_core::etag::sha256_hex(READ_ONLY.as_bytes()));

    let verified = verify(
        &origin.session(true),
        "example",
        "linkwarden",
        HEAD,
        &workflows,
    )
    .await;

    assert_eq!(outcome(verified), Ok(()));
    assert_eq!(origin.requests(), [LISTING, BLOB]);
}

#[tokio::test]
async fn a_listing_that_does_not_match_the_reviewed_set_is_refused() {
    for listed in [Reply::json(&json!([])), listing("dir")] {
        let origin = Origin::start(vec![listed]).await;
        let workflows = reviewed(af_core::etag::sha256_hex(READ_ONLY.as_bytes()));

        let verified = verify(
            &origin.session(true),
            "example",
            "linkwarden",
            HEAD,
            &workflows,
        )
        .await;

        assert_eq!(outcome(verified), Err(Some(TaskRefusal::InvalidTarget)));
        assert_eq!(origin.requests(), [LISTING]);
    }
}

#[tokio::test]
async fn a_blob_that_is_not_the_reviewed_base64_content_is_refused() {
    let encoded = BASE64_STANDARD.encode(READ_ONLY);
    for (content, encoding) in [
        (READ_ONLY, "utf-8"),
        ("%%%%", "base64"),
        (encoded.as_str(), "base64"),
    ] {
        let origin = Origin::start(vec![listing("file"), blob(content, encoding)]).await;
        let workflows = reviewed("0".repeat(64));

        let verified = verify(
            &origin.session(true),
            "example",
            "linkwarden",
            HEAD,
            &workflows,
        )
        .await;

        assert_eq!(
            outcome(verified),
            Err(Some(TaskRefusal::InvalidTarget)),
            "{content}"
        );
        assert_eq!(origin.requests(), [LISTING, BLOB]);
    }
}

#[tokio::test]
async fn a_repository_listing_more_workflows_than_were_reviewed_is_refused() {
    let origin = Origin::start(vec![Reply::json(&json!({
        "total_count": 2,
        "workflows": [{"id": 100, "path": PATH, "state": "active"}],
    }))])
    .await;
    let accepted = [ValidationWorkflow {
        path: PATH.to_owned(),
        sha256: af_core::etag::sha256_hex(READ_ONLY.as_bytes()),
    }];

    let refused = review(
        &origin.session(true),
        "example",
        "linkwarden",
        HEAD,
        "main",
        &accepted,
    )
    .await;

    assert_eq!(
        refused.err().map(|error| error.code().as_str()),
        Some("UZ-DT-004")
    );
    assert_eq!(
        origin.requests(),
        ["GET /repos/example/linkwarden/actions/workflows?per_page=17"]
    );
}
