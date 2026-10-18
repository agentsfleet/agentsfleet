//! Each review owns its account and seals credentials with the router's vault.

use super::super::{fixture::Fixture, json_body, mint, send};
use afd_api::Services as _;
use afd_auth::Scope;
use afd_core::{clock, id::Uuid7};
use afd_vault::{SecretBody, SecretName};
use http::{Method, StatusCode, header};
use serde_json::Value;

pub(super) struct Case {
    pub(super) base: Fixture,
    item: String,
}

impl Case {
    pub(super) async fn create(config: Value) -> Self {
        let base = Fixture::create().await;
        let mut request = base.request(&mint());
        request.as_object_mut().expect("request").remove("message");
        request["source"]["definition"]["config"]["x-agentsfleet"]
            .as_object_mut()
            .expect("runtime config")
            .extend(config.as_object().expect("config amendments").clone());
        let response = send(
            &base.router(),
            Method::POST,
            &base.collection(),
            Some(&base.token),
            &request.to_string(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let item = response.headers()[header::LOCATION]
            .to_str()
            .expect("draft location")
            .to_owned();
        Self { base, item }
    }

    pub(super) async fn store(&self, name: &str, body: Value) {
        let raw =
            serde_json::value::RawValue::from_string(body.to_string()).expect("credential JSON");
        self.base
            .services(&Scope::ALL)
            .secrets()
            .create(
                &self.base.workspace,
                &SecretName::parse(name).expect("credential name"),
                &SecretBody::parse(&raw).expect("credential body"),
                clock::now(),
            )
            .await
            .expect("seal credential through the router's vault");
    }

    pub(super) async fn read(&self) -> Value {
        let response = send(
            &self.base.router(),
            Method::GET,
            &self.item,
            Some(&self.base.token),
            "",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        json_body(response).await["review"].clone()
    }

    pub(super) async fn raw(&self, name: &str, plaintext: &[u8]) {
        let sealed = crate::harness::raw_secret_envelope(&self.base.workspace, name, plaintext);
        let changed = sqlx::query("UPDATE vault.secrets SET dek_ciphertext=$3,dek_nonce=$4,dek_tag=$5,secret_nonce=$6,secret_ciphertext=$7,secret_tag=$8,kek_version=$9 WHERE workspace_id=$1::uuid AND key_name=$2")
            .bind(self.base.workspace.as_str()).bind(name)
            .bind(sealed.wrapped_dek()).bind(sealed.dek_nonce().as_slice())
            .bind(sealed.dek_tag().as_slice()).bind(sealed.payload_nonce().as_slice())
            .bind(sealed.payload_ciphertext()).bind(sealed.payload_tag().as_slice())
            .bind(sealed.kek_version())
            .execute(&mut *self.base.db.acquire().await.expect("raw fixture connection"))
            .await.expect("replace owned signing bytes");
        assert_eq!(changed.rows_affected(), 1);
    }

    pub(super) async fn install(&self, team: &str, scopes: &[&str]) -> Uuid7 {
        let id = mint();
        sqlx::query("INSERT INTO connector.installs(id,provider,external_account_id,workspace_id,scopes,created_at,updated_at) VALUES($1::uuid,'slack',$2,$3::uuid,$4,now(),now())")
            .bind(id.as_str()).bind(team).bind(self.base.workspace.as_str()).bind(scopes)
            .execute(&mut *self.base.db.acquire().await.expect("install connection"))
            .await.expect("seed routing install");
        id
    }

    pub(super) async fn scopes(&self, id: &Uuid7, scopes: &[&str]) {
        let changed = sqlx::query(
            "UPDATE connector.installs SET scopes=$2 WHERE id=$1::uuid AND workspace_id=$3::uuid",
        )
        .bind(id.as_str())
        .bind(scopes)
        .bind(self.base.workspace.as_str())
        .execute(&mut *self.base.db.acquire().await.expect("scope connection"))
        .await
        .expect("change owned install scopes");
        assert_eq!(changed.rows_affected(), 1);
    }

    pub(super) async fn remove_install(&self, id: &Uuid7) {
        let removed = sqlx::query(
            "DELETE FROM connector.installs WHERE id=$1::uuid AND workspace_id=$2::uuid",
        )
        .bind(id.as_str())
        .bind(self.base.workspace.as_str())
        .execute(
            &mut *self
                .base
                .db
                .acquire()
                .await
                .expect("remove install connection"),
        )
        .await
        .expect("remove owned routing install");
        assert_eq!(removed.rows_affected(), 1);
    }

    pub(super) async fn cleanup(self) {
        self.base.cleanup().await;
    }
}

pub(super) fn check(review: &Value, kind: &str, subject: &str, state: &str) {
    let found = review["readiness"]
        .as_array()
        .expect("readiness checks")
        .iter()
        .find(|check| check["kind"] == kind && check["subject"] == subject)
        .unwrap_or_else(|| panic!("missing {kind}/{subject} check: {review}"));
    assert_eq!(found["state"], state, "{kind}/{subject}");
    assert!(
        found["observed_at"]
            .as_str()
            .is_some_and(|at| at.parse::<jiff::Timestamp>().is_ok())
    );
    assert!(!review.to_string().contains(super::TOKEN));
}
