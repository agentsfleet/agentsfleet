//! Real broker, encrypted App/installation credentials and local external GitHub exchange.
use super::{fixture::Decisions, github_server::Server};
use crate::report_seed::FIXTURE_KEK_HEX;
use afd_core::clock;
use afd_credential::credential::platform::Platform;
use afd_credential::credential::{Broker, Vendors};
use afd_credential::secrets::connector::Registry;
use afd_crypto::secret::Kek;
use afd_db::test_util::mint_id;
use afd_fleet::task::{Github, Tasks};
use afd_vault::{SecretBody, SecretName};
use serde_json::{json, value::RawValue};
use std::sync::Arc;

pub(super) async fn tasks(fixture: &Decisions<'_>, server: &Server) -> Tasks {
    configured(fixture, server, afd_fleet_runtime::config::Access::Read).await
}

pub(super) async fn publisher(fixture: &Decisions<'_>, server: &Server) -> Tasks {
    configured(fixture, server, afd_fleet_runtime::config::Access::Write).await
}

async fn configured(
    fixture: &Decisions<'_>,
    server: &Server,
    access: afd_fleet_runtime::config::Access,
) -> Tasks {
    let case = fixture.case;
    // Each installation belongs to one workspace, including concurrent fixture cases.
    let installation = installation();
    install(fixture, installation).await;
    let config = json!({"name":"pull-request-repair","x-agentsfleet":{"tools":[],"triggers":[],
        "budget":{"daily_nanos":"1000000000"},"decision_policy":super::fixture::policy(),
        "repositories":["example/linkwarden"],
        "repository_access":"write","repository_base":"main","repository_publication":"existing_pr",
        "repository_validation_workflows":[{"path":".github/workflows/test.yml","sha256":afd_core::etag::sha256_hex(WORKFLOW.as_bytes())}]}});
    case.held
        .fixtures
        .native_config(&case.held.fleet, &config.to_string())
        .await;
    let mut plane = case.held.fixtures.plane();
    let platform = platform(fixture, &plane.vault, server, installation, access).await;
    plane.broker = Arc::new(Broker::new(
        Arc::new(Registry::default()),
        Arc::new(Vendors::new(platform, server.client.clone())),
    ));
    let mut dependencies = fixture.dependencies.clone();
    dependencies.github = Some(Github::new(
        plane,
        server.client.clone(),
        Arc::new(Kek::from_hex(FIXTURE_KEK_HEX).expect("fixture key")),
    ));
    Tasks::new(
        case.held.leases.clone(),
        case.admissions.clone(),
        dependencies,
    )
}

async fn platform(
    fixture: &Decisions<'_>,
    vault: &afd_credential::vault::Vault,
    server: &Server,
    installation: u32,
    access: afd_fleet_runtime::config::Access,
) -> Platform {
    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_RSA_SHA256).expect("generated App RSA key");
    server.app(
        installation,
        &key.public_key_pem(),
        access == afd_fleet_runtime::config::Access::Write,
    );
    let raw = RawValue::from_string(
        json!({"app_id":"7","private_key_pem":key.serialize_pem()}).to_string(),
    )
    .expect("operator App credential");
    fixture
        .vault
        .create(
            &fixture.case.scope.workspace_id,
            &SecretName::parse("github-app").expect("App bag name"),
            &SecretBody::parse(&raw).expect("App secret object"),
            clock::now(),
        )
        .await
        .expect("encrypted platform App credential");
    let platform = Platform::load(vault, &fixture.case.scope.workspace_id).await;
    assert_eq!(platform.github().map(|app| app.app_id), Some(7));
    platform
}

fn installation() -> u32 {
    let mut bytes = [0; 4];
    afd_crypto::entropy::Entropy::new()
        .fill(&mut bytes)
        .expect("fixture entropy");
    u32::from_be_bytes(bytes).max(1)
}

async fn install(fixture: &Decisions<'_>, installation: u32) {
    let case = fixture.case;
    let raw = RawValue::from_string(
        json!({"integration":"github","installation_id":installation}).to_string(),
    )
    .expect("handle");
    fixture
        .vault
        .create(
            &case.scope.workspace_id,
            &SecretName::parse("github").expect("name"),
            &SecretBody::parse(&raw).expect("body"),
            clock::now(),
        )
        .await
        .expect("encrypted handle");
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("authority fixture");
    sqlx::query("INSERT INTO connector.installs (id,provider,external_account_id,workspace_id,scopes,created_at,updated_at) VALUES ($1::uuid,'github',$2,$3::uuid,'{}',now(),now())")
        .bind(mint_id()).bind(installation.to_string()).bind(case.scope.workspace_id.as_str()).execute(&mut *connection).await.expect("workspace-owned installation");
    sqlx::query("INSERT INTO connector.integration_grants (id,fleet_id,service,status,requested_reason,created_at,updated_at) VALUES ($1::uuid,$2::uuid,'github','approved','fixture',now(),now())")
        .bind(mint_id()).bind(&case.held.fleet).execute(&mut *connection).await.expect("standing approval");
    sqlx::query("UPDATE fleet.tasks SET target=jsonb_set(target,'{installation_id}',to_jsonb($2::bigint)) WHERE fleet_id=$1::uuid")
        .bind(&case.held.fleet).bind(i64::from(installation)).execute(&mut *connection).await.expect("fixture target installation");
}

pub(super) const WORKFLOW: &str = "on: push\npermissions: read-all\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo ok\n";
