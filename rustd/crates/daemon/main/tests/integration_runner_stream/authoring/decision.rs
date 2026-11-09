//! A reviewed setup decision uses a real, release-controlled TLS provider.

mod tls;

use af_core::{clock, id::Uuid7};
use af_crypto::{entropy::Entropy, secret::Kek};
use afd_admin::test_util::DecisionModelFixture;
use afd_db::{Db, test_util::mint_id};
use afd_vault::{SecretBody, SecretName, Vault};
use axum::{Json, Router, extract::State, routing::post};
use hyper_util::{rt::TokioIo, server::conn::auto::Builder, service::TowerToHyperService};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::Semaphore;
use tokio_util::task::AbortOnDropHandle;

pub(super) struct Boundary {
    pub(super) workspace: Uuid7,
    pub(super) client: reqwest::Client,
    pub(super) endpoint: String,
    state: Arc<Requests>,
    owner: AbortOnDropHandle<()>,
}

struct Requests {
    count: AtomicUsize,
    entered: Semaphore,
    release: Semaphore,
}

impl Boundary {
    pub(super) async fn open() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("decision bind");
        let address = listener.local_addr().expect("decision address");
        let (root, acceptor) = tls::server();
        let state = Arc::new(Requests {
            count: AtomicUsize::new(0),
            entered: Semaphore::new(0),
            release: Semaphore::new(0),
        });
        let app = Router::new()
            .route("/v1/systemone", post(respond))
            .with_state(Arc::clone(&state));
        let owner = AbortOnDropHandle::new(tokio::spawn(async move {
            let (socket, _) = listener.accept().await.expect("decision connects");
            let stream = acceptor.accept(socket).await.expect("decision TLS");
            let _closed = Builder::new(hyper_util::rt::TokioExecutor::new())
                .serve_connection(TokioIo::new(stream), TowerToHyperService::new(app))
                .await;
        }));
        Self {
            workspace: mint_id(),
            client: tls::client(root, address),
            endpoint: format!("https://{}:{}/v1/systemone", tls::HOST, address.port()),
            state,
            owner,
        }
    }

    pub(super) async fn model(&self, database: &Db) -> DecisionModelFixture {
        let mut model = DecisionModelFixture::create(database)
            .await
            .expect("reviewed decision model");
        let mut details = model.model.details.clone();
        details
            .decision
            .as_mut()
            .expect("decision settings")
            .configuration
            .profile
            .endpoint
            .clone_from(&self.endpoint);
        assert!(
            model
                .models
                .update(&model.model.id, details.clone(), clock::now())
                .await
                .expect("decision endpoint")
        );
        model.model.details = details;
        model
    }

    pub(super) async fn assign(&self, database: &Db, runner: &Uuid7, model: &Uuid7) {
        let key = Kek::from_hex(crate::e2e::GOOD_KEK).expect("daemon key");
        let raw = serde_json::value::to_raw_value(
            &json!({"api_key":(["native", "decision", "fixture"].join("-"))}),
        )
        .expect("decision credential JSON");
        Vault::new(database.clone(), Arc::new(key), Entropy::new())
            .create(
                &self.workspace,
                &SecretName::parse("typesafe").expect("decision secret name"),
                &SecretBody::parse(&raw).expect("decision credential shape"),
                clock::now(),
            )
            .await
            .expect("sealed decision credential");
        sqlx::query("UPDATE runner.runners SET decision_platform_model_id=$2 WHERE id=$1")
            .bind(runner)
            .bind(model)
            .execute(&mut *database.acquire().await.expect("assignment connection"))
            .await
            .expect("assigned decision model");
    }

    pub(super) async fn arrived(&self) {
        tokio::time::timeout(super::DEADLINE, self.state.entered.acquire())
            .await
            .expect("decision request deadline")
            .expect("decision entered")
            .forget();
    }

    pub(super) fn resume(&self) {
        self.state.release.add_permits(1);
    }
    pub(super) fn count(&self) -> usize {
        self.state.count.load(Ordering::SeqCst)
    }
    pub(super) async fn close(self) {
        self.owner.abort();
        let _closed = self.owner.await;
    }
}

async fn respond(State(state): State<Arc<Requests>>, Json(_body): Json<Value>) -> Json<Value> {
    state.count.fetch_add(1, Ordering::SeqCst);
    state.entered.add_permits(1);
    state
        .release
        .acquire()
        .await
        .expect("release decision response")
        .forget();
    Json(
        json!({"model":"evaluated-fixture-model","usage":{"input_tokens":100,"output_tokens":3},
        "answers":{"setup_edit":{"type":"noul","noul":0.9}}}),
    )
}

pub(super) async fn finish_model(database: &Db, runner: &Uuid7, model: DecisionModelFixture) {
    sqlx::query("UPDATE runner.runners SET decision_platform_model_id=NULL WHERE id=$1")
        .bind(runner)
        .execute(&mut *database.acquire().await.expect("unassign connection"))
        .await
        .expect("remove decision assignment");
    model.finish().await.expect("remove decision model");
}
