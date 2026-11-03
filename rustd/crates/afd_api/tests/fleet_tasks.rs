//! Task reads prove authentication, paging and dependency failure at served URLs.
#![cfg(feature = "test-util")]

use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code;
use http::{Method, StatusCode};

use crate::harness::{Fleet, OWNED_WORKSPACE, code_of, send};

const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";
const TASK: &str = "01924f4e-0000-7000-8000-00000000da71";

fn collection() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}/tasks")
}

#[tokio::test]
async fn task_reads_require_authentication_and_fleet_read_before_storage() {
    let token = format!("agt_t{}", "cafe".repeat(16));
    let router = Fleet::new()
        .with_person(
            &token,
            "user_task_reader",
            ScopeSet::from_scopes(&[Scope::ScheduleRead]),
        )
        .router();
    for path in [collection(), format!("{}/{TASK}", collection())] {
        let anonymous = send(&router, Method::GET, &path, None, "").await;
        assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
        let insufficient = send(&router, Method::GET, &path, Some(&token), "").await;
        assert_eq!(insufficient.status(), StatusCode::FORBIDDEN);
    }
}

#[tokio::test]
async fn task_input_refusals_and_storage_outages_remain_distinct_and_bounded() {
    let token = format!("agt_t{}", "face".repeat(16));
    let router = Fleet::new()
        .with_person(
            &token,
            "user_task_reader",
            ScopeSet::from_scopes(&[Scope::FleetRead]),
        )
        .router();
    let collection = collection();
    let mut invalid = [
        "limit=0",
        "limit=101",
        "limit=NaN",
        "limit=%FF",
        "starting_after=broken",
        "starting_after=%FF",
    ]
    .map(|query| format!("{collection}?{query}"))
    .to_vec();
    invalid.push(format!("{collection}?starting_after={}", "a".repeat(1025)));
    invalid.push(format!("{collection}/not-a-task"));
    for cursor in [
        afd_core::paging::Cursor::Text {
            value: "title".into(),
            id: TASK.into(),
        },
        afd_core::paging::Cursor::Timestamp {
            at: jiff::Timestamp::UNIX_EPOCH,
            id: "not-a-task".into(),
        },
    ] {
        invalid.push(format!("{collection}?starting_after={cursor}"));
    }
    let checked = async {
        for path in invalid {
            let response = send(&router, Method::GET, &path, Some(&token), "").await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
            assert_eq!(
                code_of(response).await,
                error_code::INVALID_REQUEST.as_str()
            );
        }
        for path in [collection.clone(), format!("{collection}/{TASK}")] {
            let response = send(&router, Method::GET, &path, Some(&token), "").await;
            assert_eq!(
                response.status(),
                StatusCode::SERVICE_UNAVAILABLE,
                "valid input reaches storage"
            );
            assert_eq!(
                code_of(response).await,
                error_code::INTERNAL_DB_UNAVAILABLE.as_str()
            );
        }
    };
    let completed = tokio::time::timeout(std::time::Duration::from_secs(5), checked).await;
    assert!(
        completed.is_ok(),
        "a missing datastore cannot leave a task read stuck"
    );
}
