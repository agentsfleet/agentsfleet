//! A §6 scenario: a fixture bundle installed into a booted daemon.
//!
//! Where `e2e::scenario` seeds a fleet row with an empty tool list, this goes
//! through the install verb (`afd_fleet_lifecycle::Fleets::install`) with the
//! bundle's own `TRIGGER.md` and `SKILL.md`, so the stored configuration, the
//! grant install writes for a mintable credential, and the policy every lease
//! compiles from them are the production ones. The credentials the bundle
//! declares are sealed into the vault first, because install refuses a bundle
//! naming one the workspace lacks.
//!
//! The daemon boots through `boot_with_exchanger`, so a GitHub mint runs the
//! grant check, the vault read and the broker, and only the token endpoint is
//! this file's [`MintedToken`]. The result is an ordinary [`Scenario`]: every
//! reader in `e2e_reads` and every tenant helper works on it unchanged.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_credential::credential::{Ask, Exchanger, Minted, Outcome};
use afd_credential::secrets::connector::Connector;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_fleet_lifecycle::{Fleets, Install, LibrarySource};
use afd_runner::Runners;
use afd_vault::{SecretBody, SecretName, Vault};
use afd_wire::event::EventType;
use afd_wire::runner::NetworkPolicy;
use agentsfleetd::serve::boot_with_exchanger;
use agentsfleetd::supervisor::Supervisor;
use serde_json::Value;

use crate::e2e::{
    DATABASE_LANE_KNOB, EPHEMERAL, GOOD_KEK, READY_STREAM, Scenario, daemon_environment, lane,
    unique_ids,
};
use crate::e2e_db::scenario_database;
use crate::e2e_event::enqueue;
use crate::e2e_seed::{DEEP_POOL, enrolment, seed_model_rate, seed_platform_default, seed_wallet};
use crate::e2e_seed_keys::seed_provider_key;
use crate::support::install_subscriber;

/// The bundle corpus, from this crate: the same files every corpus suite reads.
const CORPUS: &str = "../../../tests/fixtures/fleetbundle";

/// The token every GitHub mint answers with. A suite asserts it reached the
/// upstream in `Authorization` and nowhere the model could read it.
pub(crate) const MINTED_TOKEN: &str = "ghs_fixture_minted_installation_token";

/// How long a minted token lives: an hour, as GitHub's installation tokens do.
const TOKEN_LIFETIME_MS: i64 = 60 * 60 * 1000;

/// The GitHub connection handle a workspace holds: the App installation the
/// broker mints for. Its ids are never dialled; [`MintedToken`] answers.
const GITHUB_HANDLE: &str = r#"{"integration":"github","app_id":"7","installation_id":"42"}"#;

/// GitHub's token endpoint, as the lane stands it in.
#[derive(Debug)]
pub(crate) struct MintedToken;

impl Exchanger for MintedToken {
    fn exchange<'a>(
        &'a self,
        _connector: &'a dyn Connector,
        _ask: Ask<'a>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Outcome> + Send + 'a>> {
        let expires_at_ms = afd_core::clock::now()
            .as_millis()
            .saturating_add(TOKEN_LIFETIME_MS);
        Box::pin(std::future::ready(Outcome::Ok(Minted {
            token: MINTED_TOKEN.to_owned().into(),
            expires_at_ms,
            rotated_refresh_token: None,
        })))
    }
}

/// One static credential a bundle declares: its vault name and its fields.
pub(crate) struct Secret {
    pub(crate) name: &'static str,
    pub(crate) body: Value,
}

/// Boots a daemon, installs `bundle` with `secrets` and the GitHub handle
/// sealed beside them, enrols a runner, and puts one steer on its stream.
///
/// A steer for every bundle, webhook-triggered ones included: the pull path
/// leases any event type it can name, and the scripted model, not the event
/// body, decides what each run reads. `provider_base` points the identity
/// provider at a suite's listener, for one that reads through tenant routes.
pub(crate) async fn install_bundle(
    supervisor: &mut Supervisor,
    bundle: &str,
    secrets: &[Secret],
    provider_base: Option<&str>,
) -> Scenario {
    install_subscriber();
    let exclusive = READY_STREAM.lock().await;
    let database_url = scenario_database(&lane(DATABASE_LANE_KNOB));
    let booted = boot_with_exchanger(
        &daemon_environment(&database_url, provider_base, &[]),
        EPHEMERAL,
        supervisor,
        Arc::new(MintedToken),
    )
    .await
    .expect("the lane's Postgres and Dragonfly are up");
    let now = afd_core::clock::now();
    let (_unused, workspace, tenant) = unique_ids();
    seed_workspace(&booted, &workspace, &tenant, now).await;
    seed_wallet(&booted, &tenant, DEEP_POOL, now).await;
    seed_model_rate(&booted, now).await;
    let default = seed_platform_default(&booted, &workspace, now).await;
    seed_provider_key(&booted, &workspace, now).await;
    let workspace_id = Uuid7::parse(&workspace).expect("a minted workspace id");
    seal(&booted, &workspace_id, "github", GITHUB_HANDLE, now).await;
    for secret in secrets {
        seal(
            &booted,
            &workspace_id,
            secret.name,
            &secret.body.to_string(),
            now,
        )
        .await;
    }
    let fleet = install(&booted, &workspace_id, bundle, now).await;
    let enrolled = enrol(&booted, &fleet, now).await;
    let event_id = enqueue(&booted, &fleet.to_string(), &workspace, EventType::Chat).await;
    Scenario {
        base: format!("http://{}", booted.address),
        fleet: fleet.to_string(),
        workspace,
        tenant,
        event_id,
        runner_id: enrolled.0,
        token: enrolled.1,
        seeded_at: now,
        booted,
        _default: default,
        _exclusive: exclusive,
    }
}

/// The tenant and workspace rows an install selects its tenant from.
async fn seed_workspace(
    booted: &agentsfleetd::serve::Booted,
    workspace: &str,
    tenant: &str,
    now: UnixMillis,
) {
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.tenants (id, name, created_at, updated_at)
         VALUES ($1::uuid, 'bundle-tenant', $2, $2) ON CONFLICT (id) DO NOTHING",
    )
    .bind(tenant)
    .bind(now.as_millis())
    .execute(&mut *connection)
    .await
    .expect("the tenant row must insert");
    sqlx::query(
        "INSERT INTO core.workspaces (id, tenant_id, name, created_at)
         VALUES ($1::uuid, $2::uuid, 'bundle-workspace', $3) ON CONFLICT (id) DO NOTHING",
    )
    .bind(workspace)
    .bind(tenant)
    .bind(now.as_millis())
    .execute(&mut *connection)
    .await
    .expect("the workspace row must insert");
}

/// Seals `body` into the workspace's vault under `name`, as the tenant plane's
/// secret write does.
pub(crate) async fn seal(
    booted: &agentsfleetd::serve::Booted,
    workspace: &Uuid7,
    name: &str,
    body: &str,
    now: UnixMillis,
) {
    let kek = Arc::new(Kek::from_hex(GOOD_KEK).expect("the lane key is well formed"));
    let raw = serde_json::value::RawValue::from_string(body.to_owned()).expect("a JSON body");
    Vault::new(booted.database.clone(), kek, Entropy::new())
        .create(
            workspace,
            &SecretName::parse(name).expect("a valid secret name"),
            &SecretBody::parse(&raw).expect("a secret body is a JSON object"),
            now,
        )
        .await
        .expect("the vault takes the secret");
}

/// Installs `bundle` from the corpus through the production verb.
async fn install(
    booted: &agentsfleetd::serve::Booted,
    workspace: &Uuid7,
    bundle: &str,
    now: UnixMillis,
) -> Uuid7 {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(CORPUS)
        .join(bundle);
    let read = |file: &str| {
        std::fs::read_to_string(dir.join(file)).expect("the corpus bundle is readable")
    };
    let (skill, trigger) = (read("SKILL.md"), read("TRIGGER.md"));
    let kek = Arc::new(Kek::from_hex(GOOD_KEK).expect("the lane key is well formed"));
    let request = Install {
        source: LibrarySource::InCode {
            skill_markdown: &skill,
            trigger_markdown: &trigger,
        },
        name: None,
        mention: None,
    };
    Fleets::new(
        booted.database.clone(),
        booted.queue.clone(),
        kek,
        Entropy::new(),
    )
    .install(workspace, &request, now)
    .await
    .expect("the corpus bundle installs")
    .id
}

/// Enrols a runner labelled with every tag the installed fleet requires — a
/// fleet is placed only on a runner carrying its `required_tags`, which
/// install takes from the skill's `tags` — and with no egress control, since
/// this runner reports none and the daemon withholds leases from a host that
/// cannot enforce what its policy demands. Then points the fleet's affinity at
/// it, so a ready mark another suite left behind cannot take its place.
async fn enrol(
    booted: &agentsfleetd::serve::Booted,
    fleet: &Uuid7,
    now: UnixMillis,
) -> (Uuid7, String) {
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let required: Vec<String> =
        sqlx::query_scalar("SELECT required_tags FROM core.fleets WHERE id = $1::uuid")
            .bind(fleet.as_str())
            .fetch_one(&mut *connection)
            .await
            .expect("the installed fleet's tags read");
    drop(connection);
    let mut request = enrolment();
    request.assigned_policy.network_policy = NetworkPolicy::AllowAll;
    request.labels.extend(required.into_iter().map(Cow::Owned));
    let enrolled = Runners::new(booted.database.clone(), Entropy::new())
        .register(&request, now)
        .await
        .expect("enrolment must succeed");
    afd_fleet::lease::Leases::new(
        booted.database.clone(),
        booted.queue.clone(),
        Entropy::new(),
    )
    .claim(fleet, &enrolled.runner_id, now, 0)
    .await
    .expect("the affinity claim runs")
    .expect("a freshly installed fleet is unclaimed");
    (enrolled.runner_id, enrolled.token.expose().to_owned())
}
