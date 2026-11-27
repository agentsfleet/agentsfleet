use super::{
    super::access::Live,
    vendor::{Reply, Vendor, key},
};
use crate::support::Fixtures;
use af_core::clock::FixedClock;
use af_providers::web::{Exa, Parallel};
use af_wire::web::{ExtractRequest, ExtractResponse, SearchRequest, SearchResponse, WEB_DEADLINE};
use afd_db::test_util::{WorkspaceSeed, mint_id};
use afd_fleet::run::RunService;
use serde_json::json;
use std::sync::Arc;
use tokio::time::Instant;

pub(super) struct Scenario {
    pub(super) live: Live,
    pub(super) plane: RunService,
    pub(super) vendor: Vendor,
    pub(super) clock: Arc<FixedClock>,
    pub(super) fence: u64,
}

impl Scenario {
    #[expect(
        clippy::disallowed_methods,
        reason = "the fake client connects only to the two local test endpoints with retry disabled"
    )]
    pub(super) async fn new(f: &Fixtures, keys: &[&str], replies: Vec<Reply>) -> Self {
        let live = Live::issue(f).await;
        let run = live.run_id();
        let vendor = Vendor::listen(
            f.database.clone(),
            run.clone(),
            live.ready.fleet.clone(),
            replies,
        )
        .await;
        let admin = mint_id();
        let mut connection = f
            .database
            .acquire()
            .await
            .expect("create separate platform workspace");
        WorkspaceSeed::new(&admin, &live.ready.tenant)
            .insert(&mut *connection)
            .await
            .expect("platform workspace");
        drop(connection);
        for name in keys {
            f.store_secret(&admin, name, &json!({"api_key":key(&run,name)}))
                .await;
        }
        // A tenant's identically named bag must never become the web vendor key.
        for name in ["parallel", "exa"] {
            f.store_secret(
                &live.workspace,
                name,
                &json!({"api_key":key(&live.workspace,name)}),
            )
            .await;
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .expect("local vendor client");
        let parallel = Parallel::with_endpoints(
            client.clone(),
            vendor.endpoint("parallel/search"),
            vendor.endpoint("parallel/extract"),
        );
        let exa = Exa::with_endpoint(client, vendor.endpoint("exa/search"));
        let mut plane = f.plane();
        let clock = Arc::new(FixedClock::at(f.enrolled_at()));
        plane.runs = plane.runs.with_web_workspace(Some(admin)).with_web_clients(
            parallel,
            exa,
            clock.clone(),
        );
        let fence = live.payload["fencing_token"]
            .as_u64()
            .expect("issued fence");
        f.set_network(&live.ready.fleet, "public", false).await;
        Self {
            live,
            plane,
            vendor,
            clock,
            fence,
        }
    }
    pub(super) async fn search_until(
        &self,
        deadline: Instant,
    ) -> afd_fleet::Result<SearchResponse> {
        let request = garde::Unvalidated::new(SearchRequest {
            fencing_token: self.fence,
            call_number: 1,
            query: "release notes".to_owned(),
        })
        .validate()
        .expect("valid web search");
        self.plane
            .web_search(
                &self.live.ready.runner,
                self.live.run_id(),
                &request,
                deadline,
            )
            .await
    }
    pub(super) async fn search(&self) -> afd_fleet::Result<SearchResponse> {
        self.search_until(Instant::now() + WEB_DEADLINE).await
    }
    pub(super) async fn extract(&self, url: &str) -> afd_fleet::Result<ExtractResponse> {
        let request = garde::Unvalidated::new(ExtractRequest {
            fencing_token: self.fence,
            call_number: 2,
            url: url.to_owned(),
        })
        .validate()
        .expect("valid extract body");
        self.plane
            .web_extract(
                &self.live.ready.runner,
                self.live.run_id(),
                &request,
                Instant::now() + WEB_DEADLINE,
            )
            .await
    }
    pub(super) async fn reservations(&self, f: &Fixtures) -> i64 {
        let mut connection = f.database.acquire().await.expect("ledger connection");
        sqlx::query_scalar(
            "SELECT COALESCE(SUM(call_count),0)::bigint FROM thread.run_web_calls WHERE run_id=$1",
        )
        .bind(self.live.run_id())
        .fetch_one(&mut *connection)
        .await
        .expect("reservation count")
    }
}
