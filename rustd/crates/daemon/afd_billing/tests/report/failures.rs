use super::fixture::{self, Fixture, Line};
use afd_billing::tenant::Billing;
use afd_core::env::MapEnv;
use afd_core::error_code;
use afd_db::config::PoolConfig;
use afd_db::test_util::TestDatabase;
use afd_db::{Db, DbRole};
use sqlx::ConnectOptions as _;
use sqlx::postgres::PgConnectOptions;
use std::net::SocketAddr;
use std::time::Duration;

#[path = "cut_proxy.rs"]
mod cut_proxy;

const CONNECTION_BUDGET: Duration = Duration::from_millis(400);

const CUT_POINTS: [&[u8]; 5] = [
    b"BEGIN",
    b"SET TRANSACTION",
    b"WITH scoped AS",
    b"WITH grouped AS",
    b"COMMIT",
];

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn connection_loss_at_each_report_statement_keeps_money_and_retry_intact() {
    let f = Fixture::shared().await;
    fixture::seed(&f.admin, &[Line::setup(&f.tenant, 17)]).await;
    let before = fixture::ledger(&f.admin, &f.tenant).await;
    let options = TestDatabase::shared()
        .api_url()
        .parse::<PgConnectOptions>()
        .unwrap();
    let target = tokio::net::lookup_host((options.get_host(), options.get_port()))
        .await
        .unwrap()
        .find(SocketAddr::is_ipv4)
        .expect("the lane exposes IPv4 Postgres");
    for (ordinal, trigger) in CUT_POINTS.iter().enumerate() {
        let proxy = cut_proxy::CutProxy::new(target, trigger).await;
        let config = config_through(&options, proxy.addr());
        let db = Db::connect(&config).await.unwrap();
        let error = Billing::new(db.clone())
            .report(&f.tenant, &fixture::request(10))
            .await
            .unwrap_err();
        assert!(
            error.is_datastore_unavailable(),
            "boundary {}: {error}",
            ordinal + 1
        );
        assert_eq!(proxy.cuts(), 1, "boundary {} fires once", ordinal + 1);
        assert_eq!(fixture::ledger(&f.admin, &f.tenant).await, before);
        let retry = f
            .billing
            .report(&f.tenant, &fixture::request(10))
            .await
            .unwrap();
        assert_eq!(fixture::summary(&retry).charged_nanos, 17);
        assert_eq!(retry.activities.len(), 1);
        db.close().await;
        proxy.close().await;
        println!(
            "report_boundary={} connection_loss=true ledger_unchanged=true retry_total=17",
            ordinal + 1
        );
    }
    f.close().await;
}

fn config_through(options: &PgConnectOptions, addr: SocketAddr) -> PoolConfig {
    let url = options
        .clone()
        .host(&addr.ip().to_string())
        .port(addr.port())
        .to_url_lossy();
    let timeout = afd_core::clock::saturating_millis(CONNECTION_BUDGET).to_string();
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
    fixture::seed(&f.admin, &[Line::setup(&f.tenant, 17)]).await;
    f.api.close().await;
    let error = f
        .billing
        .report(&f.tenant, &fixture::request(10))
        .await
        .unwrap_err();
    assert!(error.is_datastore_unavailable());
    assert_eq!(error.code(), error_code::INTERNAL_DB_UNAVAILABLE);
    assert_eq!(fixture::ledger(&f.admin, &f.tenant).await, (1, 17, 1));
    f.close().await;
}
