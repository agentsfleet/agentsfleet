//! A cron trigger a model proposes in chat, once saved, is the fleet's schedule.
//!
//! The chat path end to end on the daemon's side: the runner's proposal crosses
//! the authoring lease exactly as a model's tool call would, the person saves
//! the reviewed candidate, and the saved fleet answers with the schedule its
//! trigger declared. The drawer's own edit path to the same schedule is
//! `revisions::schedules::fixture`.

use afd_auth::{credential::Presented, directory::Digest};
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

use super::super::{attempt, json_body, mint, send, send_with_headers};
use super::fixture::Case;

/// The schedule the proposal declares.
const SCHEDULE: &str = "0 9 * * *";
/// The zone it is read in.
const TIMEZONE: &str = "UTC";
/// What each scheduled run is told it is for.
const MESSAGE: &str = "Morning review";
/// The instance name the person saves under.
const INSTANCE: &str = "proposed-cron";

/// The draft's body, which the person reads and edits through.
async fn draft(case: &Case, item: &str) -> Value {
    let response = send(&case.router, Method::GET, item, Some(&case.base.token), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

/// The runner's terminal report for the turn its proposal belonged to. Save
/// refuses a draft whose turn is still authoring, exactly as the person's
/// client would see mid-turn, so the turn ends before the person saves.
async fn finish_turn(case: &Case, lease: &str, token: &str) {
    let usage = json!({"knowledge":"known","calls":0,"input_tokens":0,"output_tokens":0,
        "runtime_ms":0,"provider_cost_nanos":"0"});
    let report = json!({"kind":"terminal","fence":"1","deletion_generation":"0",
        "report_id":mint(),"outcome":"completed","usage":usage,"answer":"Scheduled."});
    let finished = send(
        &case.router,
        Method::POST,
        &format!("/v1/runners/me/authoring-leases/{lease}/reports"),
        Some(token),
        &report.to_string(),
    )
    .await;
    let status = finished.status();
    let finished = json_body(finished).await;
    assert_eq!(status, StatusCode::OK, "{finished}");
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_a_proposed_cron_trigger_is_saved_as_a_schedule() {
    let mut case = Case::create().await;
    case.fund().await;

    // An idle draft first: the authoring model and instance name are replaced
    // only while no turn is pending, so the message that lets a runner hold a
    // lease is sent after the PATCH, not with the create.
    let mut request = case.base.request(&mint());
    request
        .as_object_mut()
        .expect("request object")
        .remove("message");
    let created = send(
        &case.router,
        Method::POST,
        &case.base.collection(),
        Some(&case.base.token),
        &request.to_string(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let item = created.headers()[header::LOCATION]
        .to_str()
        .expect("private location")
        .to_owned();
    let tag = created.headers()[header::ETAG]
        .to_str()
        .expect("draft tag")
        .to_owned();
    let named = json!({"runtime_model_ref": case.base.model, "instance_name": INSTANCE});
    let response = send_with_headers(
        &case.router,
        Method::PATCH,
        &item,
        Some(&case.base.token),
        &named.to_string(),
        &[(header::IF_MATCH, &tag)],
    )
    .await;
    let status = response.status();
    let before = json_body(response).await;
    assert_eq!(status, StatusCode::OK, "{before}");

    // The pending message the runner's lease is for.
    let message = json!({"message": "Add a morning schedule", "operation_id": mint()});
    let sent = send(
        &case.router,
        Method::POST,
        &format!("{item}/messages"),
        Some(&case.base.token),
        &message.to_string(),
    )
    .await;
    let status = sent.status();
    let sent = json_body(sent).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{sent}");

    // The runner's lease, then its proposal: one edit adding a cron trigger.
    let runner = mint();
    case.base.own_runner(&runner);
    let token = format!("agt_r{}{}", mint(), mint()).replace('-', "");
    let lease = mint();
    let digest = Digest::of(&Presented::new(&token).expect("fixture runner token"));
    let draft_id = before["id"].as_str().expect("draft id");
    attempt::seed(&case.base, &runner, &lease, draft_id, digest.as_str()).await;

    let proposal = json!({"fence":"1","deletion_generation":"0","sequence":"1","activity_sequence":"1",
        "event":{"kind":"proposal","proposal":{"kind":"edit",
            "base_candidate_id": before["candidate"]["id"],
            "base_candidate_revision": before["candidate"]["revision"],
            "config_ops":[{"op":"add","path":"/config/x-agentsfleet/triggers",
                "value":[{"type":"cron","schedule":SCHEDULE,"timezone":TIMEZONE,"message":MESSAGE}]}],
            "instruction_edits":[]}}});
    let activity = format!("/v1/runners/me/authoring-leases/{lease}/activity");
    let proposed = send(
        &case.router,
        Method::POST,
        &activity,
        Some(&token),
        &proposal.to_string(),
    )
    .await;
    let status = proposed.status();
    let proposed = json_body(proposed).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{proposed}");

    finish_turn(&case, &lease.to_string(), &token).await;

    // The person saves the candidate the proposal committed.
    let reviewed = draft(&case, &item).await;
    let save = json!({"source":{"kind":"draft","draft_candidate_id":reviewed["candidate"]["id"]},
        "name":INSTANCE, "review_digest":reviewed["review"]["review_digest"],"start":{"kind":"none"}});
    let response = case.save(mint().as_str(), &save).await;
    let status = response.status();
    let saved = json_body(response).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");

    let schedules = saved["schedules"].as_array().expect("schedules");
    assert_eq!(
        schedules.len(),
        1,
        "one cron trigger, one schedule: {saved}"
    );
    assert_eq!(schedules[0]["cron"], SCHEDULE, "{saved}");

    case.cleanup().await;
}
