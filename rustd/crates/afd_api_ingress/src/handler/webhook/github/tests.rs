//! The classification decisions, against deliveries GitHub actually sends.
//!
//! Drives the fixture corpus rather than hand-built JSON: the point of the
//! digest is that it reads a real eighty-field payload, and a fixture written
//! to match the reader proves nothing about that.

#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::needless_pass_by_value,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_core::clock::UnixMillis;

use super::{Ingest, Policy, classify};

/// The header GitHub names a workflow run in.
const EVENT_WORKFLOW_RUN: &str = "workflow_run";

/// The header GitHub names a pull request in.
const EVENT_PULL_REQUEST: &str = "pull_request";

/// A fixed receipt instant, so a digest is byte-comparable across runs.
///
/// 2026-08-26T12:00:00Z.
const RECEIVED_AT: UnixMillis = UnixMillis::from_millis(1_787_745_600_000);

/// A failed run — the delivery that wakes a fleet.
const RUN_FAILURE: &str =
    include_str!("../../../../../../../tests/fixtures/webhooks/github_run_failure.json");

/// A `ping` — the delivery GitHub sends when a hook is first created.
const PING_DELIVERY: &str =
    include_str!("../../../../../../../tests/fixtures/webhooks/github_ping.json");

/// The same run, green.
const RUN_SUCCESS: &str =
    include_str!("../../../../../../../tests/fixtures/webhooks/github_run_success.json");

fn accepted(ingest: Ingest) -> String {
    match ingest {
        Ingest::Accept(body) => body,
        Ingest::Ignore(reason) => panic!("classified as ignored ({reason}), not accepted"),
        Ingest::Unsupported => panic!("classified as unsupported, not accepted"),
    }
}

fn ignored(ingest: Ingest) -> &'static str {
    match ingest {
        Ingest::Ignore(reason) => reason,
        Ingest::Accept(_) => panic!("classified as accepted, not ignored"),
        Ingest::Unsupported => panic!("classified as unsupported, not ignored"),
    }
}

#[test]
fn a_failed_run_becomes_the_digest_a_fleet_reasons_over() {
    let ingest = classify(
        Policy::Manual,
        EVENT_WORKFLOW_RUN,
        RUN_FAILURE.as_bytes(),
        RECEIVED_AT,
    )
    .expect("a delivery GitHub sent");

    let digest: serde_json::Value =
        serde_json::from_str(&accepted(ingest)).expect("the digest is JSON");

    // Field NAMES are the contract a fleet's prose reads.
    assert_eq!(digest["conclusion"], "failure");
    assert_eq!(digest["repo"], "example/platform");
    assert_eq!(digest["head_branch"], "main");
    assert_eq!(digest["received_at"], "2026-08-26T12:00:00Z");
    assert!(digest.get("run_url").is_some());
    assert!(digest.get("head_sha").is_some());
    assert!(digest.get("workflow_name").is_some());
    assert!(digest.get("run_id").is_some());
    assert!(digest.get("attempt").is_some());
}

#[test]
fn the_run_digest_carries_nine_fields_and_no_more() {
    // A digest that grew a field would be an eighty-field payload leaking into
    // a prompt one release at a time. The count is the guard.
    let ingest = classify(
        Policy::Manual,
        EVENT_WORKFLOW_RUN,
        RUN_FAILURE.as_bytes(),
        RECEIVED_AT,
    )
    .expect("readable");
    let digest: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&accepted(ingest)).expect("an object");

    assert_eq!(
        digest.len(),
        9,
        "keys: {:?}",
        digest.keys().collect::<Vec<_>>()
    );
}

#[test]
fn a_green_run_is_dropped_rather_than_woken_on() {
    let ingest = classify(
        Policy::Manual,
        EVENT_WORKFLOW_RUN,
        RUN_SUCCESS.as_bytes(),
        RECEIVED_AT,
    )
    .expect("readable");

    assert_eq!(ignored(ingest), "non_failure_conclusion");
}

#[test]
fn a_run_on_the_repairers_own_branch_is_dropped_before_its_conclusion_is_read() {
    // Ordering is the point: a failed run on a repair branch reports the LOOP,
    // not the failure, or the fleet investigates what it just wrote.
    let body = RUN_FAILURE.replace("\"main\"", "\"agentsfleet-repair/run-1\"");

    let ingest = classify(
        Policy::Manual,
        EVENT_WORKFLOW_RUN,
        body.as_bytes(),
        RECEIVED_AT,
    )
    .expect("readable");

    assert_eq!(ignored(ingest), "repair_branch");
}

#[test]
fn an_in_progress_run_is_dropped_as_an_action_rather_than_a_conclusion() {
    let body = RUN_FAILURE.replace("\"completed\"", "\"in_progress\"");

    let ingest = classify(
        Policy::Manual,
        EVENT_WORKFLOW_RUN,
        body.as_bytes(),
        RECEIVED_AT,
    )
    .expect("readable");

    assert_eq!(ignored(ingest), "non_completed_action");
}

#[test]
fn an_event_this_daemon_serves_no_rule_for_is_unsupported_not_malformed() {
    // A `ping` is a real delivery GitHub sends on every hook creation.
    // Answering it as a bad body would make an operator think their signature
    // was wrong.
    let ingest = classify(
        Policy::Manual,
        "ping",
        PING_DELIVERY.as_bytes(),
        RECEIVED_AT,
    )
    .expect("a ping is readable");

    assert!(matches!(ingest, Ingest::Unsupported));
}

#[test]
fn a_body_that_is_not_the_event_its_header_claims_is_an_error() {
    // Never silently unsupported: one is the sender's fault and answers
    // UZ-WH-002, the other is this daemon having no rule.
    let failed = classify(
        Policy::Manual,
        EVENT_PULL_REQUEST,
        b"{\"nonsense\":true}",
        RECEIVED_AT,
    );

    assert!(
        failed.is_err(),
        "a body that is not the event its header claims is the sender's own \
         bug, and must surface as UZ-WH-002 rather than as an unsupported event"
    );
}

#[test]
fn the_two_policies_differ_only_on_the_pull_request_action() {
    // The divergence `github_app.zig` states only by being a second file. A
    // `labeled` pull request is noise to a directly-addressed fleet and is real
    // traffic to a subscription that asked for pull_request events.
    let body = pull_request_body("labeled");

    assert_eq!(
        ignored(
            classify(
                Policy::Manual,
                EVENT_PULL_REQUEST,
                body.as_bytes(),
                RECEIVED_AT
            )
            .expect("readable")
        ),
        "uninteresting_action"
    );
    let widened = classify(
        Policy::AppIngress,
        EVENT_PULL_REQUEST,
        body.as_bytes(),
        RECEIVED_AT,
    )
    .expect("readable");
    assert!(matches!(widened, Ingest::Accept(_)));
}

#[test]
fn both_policies_drop_a_pull_request_from_the_repairers_own_branch() {
    let body = pull_request_body("opened").replace("\"feature\"", "\"agentsfleet-repair/run-1\"");

    for policy in [Policy::Manual, Policy::AppIngress] {
        let ingest =
            classify(policy, EVENT_PULL_REQUEST, body.as_bytes(), RECEIVED_AT).expect("readable");
        assert_eq!(ignored(ingest), "repair_branch", "{policy:?}");
    }
}

#[test]
fn an_opened_pull_request_carries_the_twelve_field_digest() {
    let ingest = classify(
        Policy::Manual,
        EVENT_PULL_REQUEST,
        pull_request_body("opened").as_bytes(),
        RECEIVED_AT,
    )
    .expect("readable");
    let digest: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&accepted(ingest)).expect("an object");

    assert_eq!(
        digest.len(),
        12,
        "keys: {:?}",
        digest.keys().collect::<Vec<_>>()
    );
    assert_eq!(digest["action"], "opened");
    assert_eq!(digest["repo"], "example/platform");
    assert_eq!(digest["number"], 7);
    assert_eq!(digest["head_ref"], "feature");
    assert_eq!(digest["base_ref"], "main");
    assert_eq!(digest["state"], "open");
    assert_eq!(digest["author"], "octocat");
    assert_eq!(digest["draft"], false);
}

/// A pull-request delivery shaped exactly as github.com sends one.
///
/// The actor carries all eighteen fields `octocrab`'s [`Author`] requires, and
/// that is the point of the fixture rather than an accident of copying: the
/// digest is only reachable if a REAL delivery deserializes, so an abridged
/// actor here would prove the reader works on input GitHub never sends.
const PULL_REQUEST_DELIVERY: &str =
    include_str!("../../../../../../../tests/fixtures/webhooks/github_pull_request.json");

/// That delivery at `action`.
fn pull_request_body(action: &str) -> String {
    PULL_REQUEST_DELIVERY.replace("__ACTION__", action)
}

#[test]
fn a_delivery_shaped_as_github_sends_it_deserializes() {
    // The claim this fixture exists to settle: `octocrab`'s eighteen required
    // actor fields are fields github.com actually populates, so the strictness
    // is satisfied by real traffic. An actor missing one of them is the next
    // test, and it is the risk this reader carries.
    let ingest = classify(
        Policy::Manual,
        EVENT_PULL_REQUEST,
        pull_request_body("opened").as_bytes(),
        RECEIVED_AT,
    );

    assert!(ingest.is_ok(), "a real delivery must read: {ingest:?}");
}

#[test]
fn an_actor_missing_one_url_field_fails_the_whole_delivery() {
    // Documented, not hypothetical. `octocrab`'s `Author` requires nine
    // `*_url` fields; a delivery whose actor omits ONE is refused entirely
    // rather than degrading to an absent sender. github.com sends all nine, so
    // this is a tail risk — but it is the tail where a correctly-signed
    // delivery earns a 4xx, and a sender then retries it forever.
    let body = pull_request_body("opened").replace(
        "\"gists_url\": \"https://api.github.com/users/octocat/gists{/gist_id}\",\n      ",
        "",
    );

    let refused = classify(
        Policy::Manual,
        EVENT_PULL_REQUEST,
        body.as_bytes(),
        RECEIVED_AT,
    );

    assert!(
        refused.is_err(),
        "if this ever passes, octocrab loosened Author and the risk note can go"
    );
}

/// A run with no stated attempt is its first.
///
/// The serde default behind `run_attempt`. One rather than zero because GitHub
/// numbers attempts from one, so a zero here would make the first delivery of a
/// workflow run look like a retry of something that never ran.
#[test]
fn an_unstated_run_attempt_defaults_to_the_first() {
    assert_eq!(super::one(), 1);
}
