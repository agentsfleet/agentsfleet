//! Served pagination, deterministic spend ordering and unreadable ledger refusals.
use super::{MODEL, REPORT, assert_ledger, get, harness, seed};
use crate::integration_workspace_members::fixture::Members;
use afd_core::error_code;
use http::StatusCode;
use serde_json::{Value, json};

// Value: an empty account returns exact zero totals and no phantom continuation.
#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn an_empty_report_has_no_activity_or_continuation() {
    let members = Members::create().await;
    let response = get(&members, &members.john, REPORT).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = harness::json_body(response).await;
    assert_eq!(body.pointer("/totals/charged_nanos"), Some(&json!("0")));
    assert_eq!(body.pointer("/activities/items"), Some(&json!([])));
    assert_eq!(body.pointer("/activities/next_cursor"), Some(&Value::Null));
    assert_ledger(&members, &members.john, 0, 0).await;
    members.cleanup().await;
}

// Value: equal spending has stable keys and choices across more than one fleet/model.
#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn equal_spending_uses_stable_fleet_and_model_ordering() {
    let members = Members::create().await;
    let ids = seed(&members, &members.john, 4, 17).await;
    sqlx::query("UPDATE billing.usage_lines SET fleet_id=$2::uuid,fleet_name='Fleet',model=$3 WHERE id=ANY($1::uuid[])")
        .bind(ids.get(..2).expect("four settled charges")).bind(members.fleet.as_str()).bind("second-report-model")
        .execute(&mut *members.database.acquire().await.expect("a connection"))
        .await.expect("second attribution bucket");
    let response = get(&members, &members.john, REPORT).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = harness::json_body(response).await;
    for dimension in ["by_fleet", "by_model"] {
        let buckets = body
            .pointer(&format!("/spending/{dimension}"))
            .and_then(Value::as_array)
            .expect("spending buckets");
        assert_eq!(buckets.len(), 2);
        for bucket in buckets {
            assert_eq!(bucket.pointer("/totals/charged_nanos"), Some(&json!("34")));
        }
    }
    assert_eq!(body.pointer("/spending/by_fleet/0/key"), Some(&Value::Null));
    assert_eq!(
        body.pointer("/spending/by_fleet/1/key"),
        Some(&json!(members.fleet.as_str()))
    );
    assert_eq!(
        body.pointer("/spending/by_model/0/key"),
        Some(&json!(MODEL))
    );
    assert_eq!(
        body.pointer("/spending/by_model/1/key"),
        Some(&json!("second-report-model"))
    );
    assert_eq!(
        body.pointer("/filters/models"),
        Some(&json!([MODEL, "second-report-model"]))
    );
    assert_eq!(body.pointer("/filters/fleets/0/id"), Some(&Value::Null));
    assert_ledger(&members, &members.john, 4, 68).await;
    members.cleanup().await;
}

// Value: each detail cursor advances once and cannot cross tenants or filters.
#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn detail_continuations_visit_every_charge_once_and_bind_their_scope() {
    let members = Members::create().await;
    let ids = seed(&members, &members.john, 3, 17).await;
    let work = ids.first().expect("seeded work");
    for (sequence, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE billing.usage_lines SET work_id=$2::uuid,authoring_attempt_id=$2::uuid,seq=$3 WHERE id=$1::uuid")
            .bind(id).bind(work).bind(i64::try_from(sequence).expect("bounded sequence"))
            .execute(&mut *members.database.acquire().await.expect("a connection"))
            .await.expect("one activity with three physical charges");
    }
    let base =
        format!("/v1/tenants/me/billing/activities/{work}/charges?activity_type=setup&limit=1");
    let mut path = base.clone();
    let mut seen = Vec::new();
    for expected in 0..3 {
        let response = get(&members, &members.john, &path).await;
        assert_eq!(response.status(), StatusCode::OK);
        let page = harness::json_body(response).await;
        let charges = harness::items(&page);
        assert_eq!(charges.len(), 1);
        let charge = charges.first().expect("one charge");
        assert_eq!(charge.get("sequence"), Some(&json!(expected)));
        seen.push(
            harness::text(charge, "id")
                .expect("charge identifier")
                .to_owned(),
        );
        if let Some(cursor) = harness::text(&page, "next_cursor") {
            assert!(expected < 2);
            assert_refused_cursor(&members, &base, cursor).await;
            path = format!("{base}&starting_after={cursor}");
        } else {
            assert_eq!(expected, 2);
        }
    }
    assert_eq!(seen, ids);
    assert_ledger(&members, &members.john, 3, 51).await;
    members.cleanup().await;
}

async fn assert_refused_cursor(members: &Members, base: &str, cursor: &str) {
    for (person, extra) in [(&members.stranger, ""), (&members.john, "&model=other")] {
        let response = get(
            members,
            person,
            &format!("{base}{extra}&starting_after={cursor}"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let problem = harness::json_body(response).await;
        assert_eq!(
            harness::error_code(&problem),
            Some(error_code::INVALID_REQUEST.as_str())
        );
    }
}

// Value: an unreadable stored work kind is an internal failure without ledger mutation.
#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn unreadable_stored_work_is_refused_without_disclosing_or_changing_it() {
    let members = Members::create().await;
    let ids = seed(&members, &members.john, 1, 17).await;
    sqlx::query("UPDATE billing.usage_lines SET work_type='unreadable-private-work' WHERE id=ANY($1::uuid[])")
        .bind(&ids).execute(&mut *members.database.acquire().await.expect("a connection"))
        .await.expect("controlled stored-row failure");
    let response = get(&members, &members.john, REPORT).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let problem = harness::json_body(response).await;
    assert_eq!(
        harness::error_code(&problem),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert!(!problem.to_string().contains("unreadable-private-work"));
    assert_ledger(&members, &members.john, 1, 17).await;
    members.cleanup().await;
}
