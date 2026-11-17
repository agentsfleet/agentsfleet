use super::cut_proxy::CutProxy;
use super::fixture::{self, Fixture, Line};
use af_core::env::MapEnv;
use af_core::error_code;
use afd_billing::tenant::Billing;
use afd_db::config::PoolConfig;
use afd_db::test_util::TestDatabase;
use afd_db::{Db, DbRole};
use sqlx::ConnectOptions as _;
use sqlx::postgres::PgConnectOptions;
use std::net::SocketAddr;
use std::time::Duration;

const CONNECTION_BUDGET: Duration = Duration::from_millis(400);

/// The report is one statement over the rollup; this fragment opens it.
const REPORT_STATEMENT: &[u8] = b"WITH cells AS";

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn connection_loss_during_the_report_read_keeps_money_and_retry_intact() {
    let f = Fixture::shared().await;
    fixture::record(&f, &[Line::setup(&f.tenant, 17)]).await;
    let before = fixture::written(&f.admin, &f.tenant).await;
    let (options, target) = api_target().await;
    let proxy = CutProxy::new(target, REPORT_STATEMENT).await;
    let db = Db::connect(&config_through(&options, proxy.addr()))
        .await
        .unwrap();
    let error = Billing::new(db.clone())
        .report(&f.tenant, &fixture::request())
        .await
        .unwrap_err();
    assert!(error.is_datastore_unavailable(), "{error}");
    assert!(error.is_unanswered() && !error.is_contended(), "{error}");
    assert_eq!(proxy.cuts(), 1, "the report statement is cut once");
    assert_eq!(fixture::written(&f.admin, &f.tenant).await, before);
    let retry = f
        .billing
        .report(&f.tenant, &fixture::request())
        .await
        .unwrap();
    assert_eq!(
        (
            fixture::summary(&retry).charged_nanos,
            fixture::summary(&retry).line_count
        ),
        (17, 1)
    );
    db.close().await;
    proxy.close().await;
    println!(
        "report_boundary=aggregates connection_loss=true ledger_unchanged=true retry_total=17"
    );
    f.close().await;
}

/// The API login's connect options and the lane's IPv4 Postgres address.
pub(super) async fn api_target() -> (PgConnectOptions, SocketAddr) {
    let options = TestDatabase::shared()
        .api_url()
        .parse::<PgConnectOptions>()
        .unwrap();
    let target = tokio::net::lookup_host((options.get_host(), options.get_port()))
        .await
        .unwrap()
        .find(SocketAddr::is_ipv4)
        .expect("the lane exposes IPv4 Postgres");
    (options, target)
}

/// An API pool that reaches Postgres only through `addr`.
pub(super) fn config_through(options: &PgConnectOptions, addr: SocketAddr) -> PoolConfig {
    let url = options
        .clone()
        .host(&addr.ip().to_string())
        .port(addr.port())
        .to_url_lossy();
    let timeout = af_core::clock::saturating_millis(CONNECTION_BUDGET).to_string();
    let env = MapEnv::from_pairs([
        (DbRole::Api.url_knob(), url.as_str()),
        ("DATABASE_ACQUIRE_TIMEOUT_MS", timeout.as_str()),
    ]);
    PoolConfig::resolve(&env, DbRole::Api)
        .expect("the fixture URL is valid")
        .with_connect_timeout(CONNECTION_BUDGET)
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn a_closed_report_pool_returns_an_unavailable_error_without_changing_the_ledger() {
    let f = Fixture::shared().await;
    fixture::record(&f, &[Line::setup(&f.tenant, 17)]).await;
    let before = fixture::written(&f.admin, &f.tenant).await;
    f.api.close().await;
    let error = f
        .billing
        .report(&f.tenant, &fixture::request())
        .await
        .unwrap_err();
    assert!(error.is_datastore_unavailable());
    assert_eq!(error.code(), error_code::INTERNAL_DB_UNAVAILABLE);
    assert_eq!(before.ledger, (1, 17, 1));
    assert_eq!(fixture::written(&f.admin, &f.tenant).await, before);
    f.close().await;
}
