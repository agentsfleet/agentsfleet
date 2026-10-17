//! Token limits hold at the runner route, not only in the domain call.
//!
//! The wire bound refuses before the domain is consulted. The exact remaining
//! output allowance dispatches; one token past what is left, and a context
//! window too small for the assembled prompt, each refuse with no physical
//! intent, and the refused attempt cannot be retried into a dispatch.

use super::super::{json_body, send};
use super::fixture::Model;
use afd_core::error_code;
use http::{Method, StatusCode};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

/// The per-turn output allowance a fresh attempt starts with.
const OUTPUT_PER_TURN: u32 = 8_192;
/// Output already spent before the next request, leaving 64 tokens.
const SPENT: u32 = OUTPUT_PER_TURN - 64;
/// Output tokens the fixture provider reports for one dispatch.
const PROVIDER_OUTPUT: u32 = 20;
/// A window too small for the fixed prompt the daemon assembles.
const NARROW_WINDOW: i32 = 64;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn output_limits_refuse_at_the_runner_route_and_the_exact_allowance_dispatches() {
    let model = Model::create().await;
    let past_wire = with_output(&model.request, 0);
    assert_eq!(call(&model, &past_wire).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(model.receipts().await, 0, "the wire bound wrote nothing");

    spend(&model, SPENT).await;
    let exact = with_output(&model.request, OUTPUT_PER_TURN - SPENT);
    let dispatched = call(&model, &exact).await;
    assert_eq!(dispatched.0, StatusCode::OK, "{}", dispatched.1);
    assert_eq!(model.provider.effects.load(Ordering::SeqCst), 1);

    let left = OUTPUT_PER_TURN - SPENT - PROVIDER_OUTPUT;
    let past_remaining = next_call(&with_output(&model.request, left + 1));
    let refused = call(&model, &past_remaining).await;
    assert_eq!(refused.0, StatusCode::UNPROCESSABLE_ENTITY, "{}", refused.1);
    assert_eq!(code(&refused.1), Some(error_code::AUTHORING_TOKEN.as_str()));
    assert_eq!(model.provider.effects.load(Ordering::SeqCst), 1);
    assert_eq!(
        model.receipts().await,
        1,
        "the refusal left no second intent"
    );
    let retried = call(&model, &next_call(&with_output(&model.request, 1))).await;
    assert_eq!(retried.0, StatusCode::CONFLICT, "{}", retried.1);
    assert_eq!(model.provider.effects.load(Ordering::SeqCst), 1);
    model.running.f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_window_too_small_for_the_prompt_refuses_at_the_runner_route() {
    let model = Model::create().await;
    window(&model, NARROW_WINDOW, NARROW_WINDOW.cast_unsigned()).await;
    let refused = call(&model, &with_output(&model.request, 1)).await;
    assert_eq!(refused.0, StatusCode::UNPROCESSABLE_ENTITY, "{}", refused.1);
    assert_eq!(code(&refused.1), Some(error_code::AUTHORING_TOKEN.as_str()));
    assert_eq!(model.provider.effects.load(Ordering::SeqCst), 0);
    assert_eq!(model.receipts().await, 0, "no physical intent");
    model.running.f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn configured_output_limits_above_eight_thousand_dispatch_and_bound_the_remaining_turn() {
    let model = Model::create().await;
    let configured = 16_384;
    window(&model, 128_000, configured).await;
    let dispatched = call(&model, &with_output(&model.request, configured)).await;
    assert_eq!(dispatched.0, StatusCode::OK, "{}", dispatched.1);
    assert_eq!(model.provider.effects.load(Ordering::SeqCst), 1);
    let refused = call(
        &model,
        &next_call(&with_output(
            &model.request,
            configured - PROVIDER_OUTPUT + 1,
        )),
    )
    .await;
    assert_eq!(refused.0, StatusCode::UNPROCESSABLE_ENTITY, "{}", refused.1);
    assert_eq!(code(&refused.1), Some(error_code::AUTHORING_TOKEN.as_str()));
    assert_eq!(model.provider.effects.load(Ordering::SeqCst), 1);
    assert_eq!(model.receipts().await, 1);
    model.running.f.cleanup().await;
}

/// The same request as a second physical dispatch of the attempt.
fn next_call(request: &Value) -> Value {
    let mut next = request.clone();
    *next.get_mut("dispatch_id").expect("dispatch identity") =
        json!(super::super::mint().to_string());
    *next.get_mut("call_index").expect("call index") = json!(2);
    next
}

fn with_output(request: &Value, tokens: u32) -> Value {
    let mut changed = request.clone();
    *changed
        .pointer_mut("/request/max_output_tokens")
        .expect("the fixture names an output bound") = json!(tokens);
    changed
}

async fn call(model: &Model, body: &Value) -> (StatusCode, Value) {
    let r = &model.running;
    let response = send(
        &r.router,
        Method::POST,
        &model.path,
        Some(&r.token),
        &body.to_string(),
    )
    .await;
    let status = response.status();
    (status, json_body(response).await)
}

fn code(problem: &Value) -> Option<&str> {
    problem.get("error_code").and_then(Value::as_str)
}

async fn spend(model: &Model, output: u32) {
    sqlx::query("UPDATE fleet.authoring_attempts SET output_tokens=$2 WHERE draft_id=$1::uuid")
        .bind(&model.running.draft)
        .bind(i64::from(output))
        .execute(&mut *model.running.f.db.acquire().await.expect("spend"))
        .await
        .expect("record spent output on the admitted attempt");
}

/// Window changes alter the live model, so the pinned snapshot follows them;
/// the case is about token bounds, not about a model swap.
async fn window(model: &Model, tokens: i32, output: u32) {
    let f = &model.running.f;
    let mut connection = f.db.acquire().await.expect("window");
    sqlx::query(
        "UPDATE core.model_library SET context_cap_tokens=$2, \
         inference=jsonb_set(inference, '{max_output_tokens}', to_jsonb($3::bigint)) WHERE id=$1::uuid",
    )
        .bind(f.catalogue.as_str())
        .bind(tokens)
        .bind(i64::from(output))
        .execute(&mut *connection)
        .await
        .expect("set the owned model's window");
    let live = afd_credential::provider::registered_model(
        &mut connection,
        &f.tenant,
        &f.model.to_string(),
    )
    .await
    .expect("registered metadata")
    .expect("owner model exists");
    sqlx::query(
        "UPDATE fleet.authoring_attempts SET model_snapshot=$2::jsonb WHERE draft_id=$1::uuid",
    )
    .bind(&model.running.draft)
    .bind(serde_json::to_string(&live).expect("metadata JSON"))
    .execute(&mut *connection)
    .await
    .expect("pin the admitted attempt to the live window");
}
