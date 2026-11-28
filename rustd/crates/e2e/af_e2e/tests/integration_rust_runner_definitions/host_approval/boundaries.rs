//! Separate listeners make destination DNS, TCP, requests and vendor dispatch independently observable.
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use af_egress::testing::upstream::Lookups;
use af_providers::web::{Exa, Parallel};
use afd_db::test_util::{GrantSeed, WorkspaceSeed, mint_id};
use agentsfleetd::{
    preflight::PLATFORM_ADMIN_WORKSPACE_KNOB, serve::boot_with_web_clients, supervisor::Supervisor,
};
use hyper::Method;
use serde_json::json;

use super::script::{DEST, GUIDE, ORIGIN, SENTINEL};
use crate::definition_install::{install_on, seal};
use crate::e2e::{DATABASE_LANE_KNOB, EPHEMERAL, SCENARIOS, Scenario, daemon_environment, lane};
use crate::https::{Reply, Route, Upstream};
use crate::integration_tenant_registry::provider_listener;

pub(super) struct Pages {
    origin: Upstream,
    destination: Upstream,
}

impl Pages {
    pub(super) async fn serve() -> Self {
        let origin = Upstream::serve(vec![Route::new(
            ORIGIN,
            Method::GET,
            "/guide",
            vec![Reply::redirect(
                302,
                &format!("https://{DEST}/migration?signature=fixture-private"),
            )],
        )])
        .await;
        let destination = Upstream::serve(vec![Route::new(
            DEST,
            Method::GET,
            "/migration",
            vec![Reply::text(200, SENTINEL)],
        )])
        .await;
        Self {
            origin,
            destination,
        }
    }

    pub(super) fn network(&self) -> (af_egress::Network, Arc<Lookups>) {
        Upstream::observed_network(&[&self.origin, &self.destination])
    }

    pub(super) fn assert_phase(
        &self,
        dns: &Lookups,
        origin_reads: usize,
        destination_reads: usize,
    ) {
        assert!(dns.count(ORIGIN) > 0 && self.origin.accepted() > 0);
        assert_eq!(self.origin.seen().len(), origin_reads);
        assert_eq!(dns.count(DEST), destination_reads);
        assert_eq!(self.destination.accepted(), destination_reads);
        let seen = self.destination.seen();
        assert_eq!(seen.len(), destination_reads);
        assert!(
            seen.iter().all(|request| request.path == "/migration"
                && request.query == "signature=fixture-private")
        );
    }
}

pub(super) struct Vendors {
    client: reqwest::Client,
    endpoint: url::Url,
    requests: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl Vendors {
    pub(super) async fn start() -> Self {
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&requests);
        let app = axum::Router::new().fallback(move || {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                hyper::StatusCode::SERVICE_UNAVAILABLE
            }
        });
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("vendor listener");
        let endpoint = url::Url::parse(&format!(
            "http://{}/",
            listener.local_addr().expect("vendor port")
        ))
        .expect("fixture URL");
        let task =
            tokio::spawn(async move { axum::serve(listener, app).await.expect("vendor server") });
        let client = afd_identity::direct(std::time::Duration::from_secs(5))
            .build()
            .expect("fixture client");
        Self {
            client,
            endpoint,
            requests,
            task,
        }
    }

    pub(super) async fn scenario(&self, supervisor: &mut Supervisor) -> Scenario {
        let admin = mint_id();
        let provider = provider_listener().await;
        let env = daemon_environment(
            &lane(DATABASE_LANE_KNOB),
            Some(&provider),
            &[(PLATFORM_ADMIN_WORKSPACE_KNOB, admin.as_str())],
        );
        let parallel = Parallel::with_endpoints(
            self.client.clone(),
            self.endpoint.join("parallel/search").expect("search"),
            self.endpoint.join("parallel/extract").expect("extract"),
        );
        let exa = Exa::with_endpoint(
            self.client.clone(),
            self.endpoint.join("exa/search").expect("search"),
        );
        agentsfleetd::test_util::install_subscriber();
        let guard = SCENARIOS.lock().await;
        let booted = boot_with_web_clients(&env, EPHEMERAL, supervisor, (parallel, exa))
            .await
            .expect("test daemon");
        let request =
            json!({"message":format!("Please read {GUIDE} and tell me the migration step.")})
                .to_string();
        let scene = install_on(booted, guard, ("delegating-triager", &[], None), &request).await;
        let mut connection = scene
            .booted
            .database
            .acquire()
            .await
            .expect("seed connection");
        WorkspaceSeed::new(&admin, &scene.tenant)
            .insert(&mut *connection)
            .await
            .expect("platform workspace");
        GrantSeed::new(&scene.fleet, ORIGIN, "host", "approved")
            .approved(scene.seeded_at)
            .insert(&mut *connection)
            .await
            .expect("origin grant");
        drop(connection);
        for name in ["parallel", "exa"] {
            seal(
                &scene.booted,
                &admin,
                name,
                r#"{"api_key":"fixture-web-key"}"#,
                scene.seeded_at,
            )
            .await;
        }
        scene
    }

    pub(super) async fn assert_idle(&self, scene: &Scenario) {
        let mut connection = scene
            .booted
            .database
            .acquire()
            .await
            .expect("read reservations");
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM thread.run_web_calls w JOIN thread.runs r ON r.id=w.run_id WHERE r.fleet_id=$1")
            .bind(&scene.fleet).fetch_one(&mut *connection).await.expect("web reservations");
        assert_eq!(count, 0);
        assert_eq!(self.requests.load(Ordering::SeqCst), 0);
    }
}

impl Drop for Vendors {
    fn drop(&mut self) {
        self.task.abort();
    }
}
