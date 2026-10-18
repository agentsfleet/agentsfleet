//! A whole-instructions edit at the character cap crosses the real runner route.
//!
//! The cap counts characters, so a two-byte character fills it with twice the
//! bytes: 32,768 of them is a 64 KiB edit inside a larger JSON body. One past
//! the cap is refused with the head unchanged.

use super::{attempt::Running, json_body, send};
use afd_fleet_runtime::definition::MAX_INSTRUCTIONS_CHARS;
use http::{Method, StatusCode};
use serde_json::{Value, json};

/// Two UTF-8 bytes, one character.
const WIDE: char = 'é';

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_instruction_edit_at_the_character_cap_crosses_the_router_and_one_past_is_refused() {
    let r = Running::create().await;
    let initialized = activity(&r, &r.initialize().to_string()).await;
    assert_eq!(initialized.0, StatusCode::ACCEPTED);
    let (base, revision) = version(&initialized.1);
    let length = instructions(&r, &base).await.len();

    let full: String = std::iter::repeat_n(WIDE, MAX_INSTRUCTIONS_CHARS).collect();
    let at_cap = activity(&r, &edit(&base, &revision, length, &full, 2)).await;
    assert_eq!(at_cap.0, StatusCode::ACCEPTED, "{}", at_cap.1);
    let (head, head_revision) = version(&at_cap.1);
    assert_eq!(head_revision, "2");
    let stored = instructions(&r, &head).await;
    assert_eq!(stored.chars().count(), MAX_INSTRUCTIONS_CHARS);
    assert_eq!(
        stored.len(),
        2 * MAX_INSTRUCTIONS_CHARS,
        "the cap is not a byte cap"
    );

    let past = format!("{full}{WIDE}");
    let refused = activity(&r, &edit(&head, &head_revision, stored.len(), &past, 3)).await;
    assert_eq!(refused.0, StatusCode::BAD_REQUEST, "{}", refused.1);
    assert_eq!(
        candidates(&r).await,
        2,
        "a refused edit commits no version and moves no head"
    );
    r.f.cleanup().await;
}

async fn activity(r: &Running, body: &str) -> (StatusCode, Value) {
    let response = send(&r.router, Method::POST, &r.path, Some(&r.token), body).await;
    let status = response.status();
    (status, json_body(response).await)
}

fn version(accepted: &Value) -> (String, String) {
    let field = |name: &str| {
        accepted
            .pointer(&format!("/candidate/{name}"))
            .and_then(Value::as_str)
            .expect("an accepted proposal names its version")
            .to_owned()
    };
    (field("id"), field("revision"))
}

fn edit(base: &str, revision: &str, end: usize, text: &str, activity: u32) -> String {
    json!({"fence":"1","deletion_generation":"0","sequence":"1",
        "activity_sequence":activity.to_string(),
        "event":{"kind":"proposal","proposal":{"kind":"edit",
            "base_candidate_id":base,"base_candidate_revision":revision,"config_ops":[],
            "instruction_edits":[{"start_byte":0,"end_byte":end,"text":text}]}}})
    .to_string()
}

async fn instructions(r: &Running, candidate: &str) -> String {
    sqlx::query_scalar(
        "SELECT definition->>'instructions_md' FROM authoring.candidates WHERE id=$1::uuid",
    )
    .bind(candidate)
    .fetch_one(&mut *r.f.db.acquire().await.expect("observer"))
    .await
    .expect("the committed version's instructions")
}

async fn candidates(r: &Running) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM authoring.candidates WHERE draft_id=$1::uuid")
        .bind(&r.draft)
        .fetch_one(&mut *r.f.db.acquire().await.expect("observer"))
        .await
        .expect("owned candidates")
}
