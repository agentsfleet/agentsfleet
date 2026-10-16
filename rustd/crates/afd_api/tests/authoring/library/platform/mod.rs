//! Native platform routes prove creator authority and explicit visibility.

mod authority;
mod idempotency;
mod install;
mod visibility;

use super::{
    Case as LibraryCase, Method, StatusCode, header, json, json_body, mint, send, send_with_headers,
};
use axum::response::Response;
use serde_json::Value;

struct Case {
    inner: LibraryCase,
    slug: String,
    staged: Value,
    tag: String,
    key: String,
}

impl Case {
    async fn create() -> Self {
        let mut inner = LibraryCase::create().await;
        let slug = format!("catalog-{}", mint());
        inner.base.own_platform_library(&slug);
        let draft = inner.get(&inner.draft).await;
        let tag = draft.headers()[header::ETAG]
            .to_str()
            .expect("draft tag")
            .to_owned();
        let patch = json!({"runtime_model_ref":inner.base.model,"config_ops":[
            {"op":"replace","path":"/config/name","value":slug}
        ]});
        let response = send_with_headers(
            &inner.router,
            Method::PATCH,
            &inner.draft,
            Some(&inner.base.token),
            &patch.to_string(),
            &[(header::IF_MATCH, &tag)],
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        inner.candidate = json_body(response).await["candidate"]["id"].clone();
        let mut case = Self {
            inner,
            slug,
            staged: Value::Null,
            tag: String::new(),
            key: mint().to_string(),
        };
        let response = case.stage(&case.body()).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        case.tag = response.headers()[header::ETAG]
            .to_str()
            .expect("staged tag")
            .to_owned();
        case.staged = json_body(response).await;
        case
    }

    fn body(&self) -> Value {
        json!({"workspace_id":self.inner.base.workspace,"draft_candidate_id":self.inner.candidate,"label":"Operator setup"})
    }

    fn admin_path(&self) -> String {
        format!("/v1/admin/fleet-libraries/{}", self.slug)
    }
    fn tenant_path(&self) -> String {
        format!("{}/{}", self.inner.collection(), self.slug)
    }

    async fn stage(&self, body: &Value) -> Response {
        send_with_headers(
            &self.inner.router,
            Method::POST,
            "/v1/admin/fleet-libraries",
            Some(&self.inner.base.token),
            &body.to_string(),
            &[(
                header::HeaderName::from_static("idempotency-key"),
                &self.key,
            )],
        )
        .await
    }

    /// One `PATCH` of the admin resource under the caller's headers, so a
    /// suite can send a missing, stale or current tag.
    async fn patch_admin(&self, body: &str, headers: &[(header::HeaderName, &str)]) -> Response {
        send_with_headers(
            &self.inner.router,
            Method::PATCH,
            &self.admin_path(),
            Some(&self.inner.base.token),
            body,
            headers,
        )
        .await
    }

    /// Deletes the admin resource under the current tag.
    async fn withdraw(&self) -> Response {
        send_with_headers(
            &self.inner.router,
            Method::DELETE,
            &self.admin_path(),
            Some(&self.inner.base.token),
            "",
            &[(header::IF_MATCH, &self.tag)],
        )
        .await
    }

    async fn curate(&mut self, body: &Value) {
        let response = self
            .patch_admin(&body.to_string(), &[(header::IF_MATCH, &self.tag)])
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        response.headers()[header::ETAG]
            .to_str()
            .expect("curated tag")
            .clone_into(&mut self.tag);
    }
}

/// Stages `body` through `router` as `token`, under a fresh idempotency key.
async fn stage_through(router: &axum::Router, token: &str, body: &Value) -> Response {
    send_with_headers(
        router,
        Method::POST,
        "/v1/admin/fleet-libraries",
        Some(token),
        &body.to_string(),
        &[(
            header::HeaderName::from_static("idempotency-key"),
            mint().as_str(),
        )],
    )
    .await
}
