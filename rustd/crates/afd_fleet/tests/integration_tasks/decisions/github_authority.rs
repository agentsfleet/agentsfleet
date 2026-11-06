//! Real standing grant, installation ownership and encrypted handle; exchange is a scoped fixture.
use super::fixture::Decisions;
use crate::report_seed::FIXTURE_KEK_HEX;
use afd_core::clock;
use afd_credential::credential::{Ask, Broker, Exchanger, Minted, Outcome};
use afd_credential::secrets::connector::{Connector, Registry};
use afd_crypto::secret::Kek;
use afd_db::test_util::mint_id;
use afd_fleet::task::{Github, Tasks};
use afd_vault::{SecretBody, SecretName};
use serde_json::{json, value::RawValue};
use std::{pin::Pin, sync::Arc, time::Duration};
use zeroize::Zeroizing;

#[derive(Debug)]
struct Reader(afd_fleet_runtime::config::Access);
impl Exchanger for Reader {
    fn exchange<'a>(
        &'a self,
        _connector: &'a dyn Connector,
        ask: Ask<'a>,
    ) -> Pin<Box<dyn Future<Output = Outcome> + Send + 'a>> {
        let binding = ask.binding.expect("native repository binding");
        assert!(
            binding.access() == afd_fleet_runtime::config::Access::Read
                || binding.access() == self.0
        );
        assert_eq!(binding.repositories().len(), 1);
        assert_eq!(binding.repositories()[0].as_ref(), "example/linkwarden");
        Box::pin(std::future::ready(Outcome::Ok(Minted {
            token: Zeroizing::new("native-test-read-token".to_owned()),
            expires_at: clock::after(clock::now(), Duration::from_secs(300)),
            rotated_refresh_token: None,
        })))
    }
}

pub(super) async fn tasks(fixture: &Decisions<'_>, client: reqwest::Client) -> Tasks {
    configured(fixture, client, afd_fleet_runtime::config::Access::Read).await
}

pub(super) async fn publisher(fixture: &Decisions<'_>, client: reqwest::Client) -> Tasks {
    configured(fixture, client, afd_fleet_runtime::config::Access::Write).await
}

async fn configured(
    fixture: &Decisions<'_>,
    client: reqwest::Client,
    access: afd_fleet_runtime::config::Access,
) -> Tasks {
    let case = fixture.case;
    // Each installation belongs to one workspace, including concurrent fixture cases.
    let installation = installation();
    install(fixture, installation).await;
    let config = json!({"name":"pull-request-repair","x-agentsfleet":{"tools":[],"triggers":[],
        "budget":{"daily_nanos":"1000000000"},"repositories":["example/linkwarden"],
        "repository_access":"write","repository_base":"main","repository_publication":"existing_pr",
        "repository_validation_workflows":[{"path":".github/workflows/test.yml","sha256":afd_core::etag::sha256_hex(WORKFLOW.as_bytes())}]}});
    case.held
        .fixtures
        .native_config(&case.held.fleet, &config.to_string())
        .await;
    let mut plane = case.held.fixtures.plane();
    plane.broker = Arc::new(Broker::new(
        Arc::new(Registry::default()),
        Arc::new(Reader(access)),
    ));
    let mut dependencies = fixture.dependencies.clone();
    dependencies.github = Some(Github::new(
        plane,
        client,
        Arc::new(Kek::from_hex(FIXTURE_KEK_HEX).expect("fixture key")),
    ));
    Tasks::new(
        case.held.leases.clone(),
        case.admissions.clone(),
        dependencies,
    )
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
