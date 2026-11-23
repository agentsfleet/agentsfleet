//! Every recipe test owns one fleet and edits through the same route as a person.

use super::*;

pub(super) struct Scheduled {
    pub(super) fleet: Fleet,
    pub(super) item: String,
}

impl Scheduled {
    pub(super) async fn create() -> Self {
        let case = Case::create().await;
        case.fund().await;
        let (draft, mut request) = case.request().await;
        let reviewed = patch_recipe(&case, &draft, "0 9 * * *", "Original base").await;
        request["source"]["draft_candidate_id"] = reviewed["candidate"]["id"].clone();
        request["review_digest"] = reviewed["review"]["review_digest"].clone();
        let response = case.save(mint().as_str(), &request).await;
        let status = response.status();
        let item = response.headers()[header::LOCATION]
            .to_str()
            .expect("fleet location")
            .to_owned();
        let saved = json_body(response).await;
        assert_eq!(status, StatusCode::CREATED, "{saved}");
        assert_eq!(saved["schedules"].as_array().expect("schedules").len(), 1);
        let schedule = saved["schedules"][0]["schedule_id"]
            .as_str()
            .expect("schedule ID");
        let schedule_item = format!("{item}/schedules/{schedule}");
        Self {
            fleet: Fleet { case, item, saved },
            item: schedule_item,
        }
    }

    pub(super) async fn read(&self) -> Value {
        let response = send(
            &self.fleet.case.router,
            Method::GET,
            &self.item,
            Some(&self.fleet.case.base.token),
            "",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let tag = response.headers()[header::ETAG]
            .to_str()
            .expect("schedule header")
            .to_owned();
        let row = json_body(response).await;
        assert_eq!(row["etag"], tag);
        row
    }

    pub(super) async fn edit(&self, tag: Option<&str>, patch: Value) -> axum::response::Response {
        let headers: Vec<_> = tag
            .into_iter()
            .map(|value| (header::IF_MATCH, value))
            .collect();
        send_with_headers(
            &self.fleet.case.router,
            Method::PATCH,
            &self.item,
            Some(&self.fleet.case.base.token),
            &patch.to_string(),
            &headers,
        )
        .await
    }

    pub(super) async fn improvement(&self) -> Value {
        let revision = &self.fleet.saved["setup_revision"];
        self.recipe_on(revision, "0 17 * * *", "Revised base").await
    }

    /// An improvement of setup `revision` whose recipe moves the schedule to
    /// `cron` and `message`, answered as the Apply body for its review.
    pub(super) async fn recipe_on(&self, revision: &Value, cron: &str, message: &str) -> Value {
        let (draft, _) = self.fleet.improvement_of(revision).await;
        let updated = patch_recipe(&self.fleet.case, &draft, cron, message).await;
        json!({"draft_candidate_id":updated["candidate"]["id"],"review_digest":updated["review"]["review_digest"]})
    }
}

async fn patch_recipe(case: &Case, draft: &str, cron: &str, message: &str) -> Value {
    let response = send(&case.router, Method::GET, draft, Some(&case.base.token), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    let tag = response.headers()[header::ETAG]
        .to_str()
        .expect("draft tag")
        .to_owned();
    let body = json!({"config_ops":[{"op":"add","path":"/config/x-agentsfleet/triggers",
        "value":[{"type":"cron","schedule":cron,"timezone":"UTC","message":message}]}]});
    let response = send_with_headers(
        &case.router,
        Method::PATCH,
        draft,
        Some(&case.base.token),
        &body.to_string(),
        &[(header::IF_MATCH, &tag)],
    )
    .await;
    let status = response.status();
    let updated = json_body(response).await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    updated
}
