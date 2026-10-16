//! The installed fleet: its native rows read from the corpus, its approved
//! GitHub grant, and the runner enrolled to lease it.

use std::borrow::Cow;
use std::path::PathBuf;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::test_util::FleetSeed;
use afd_fleet_runtime::Definition;
use afd_runner::Runners;
use afd_state::sql::FLEET_STATUS_ACTIVE;
use afd_wire::runner::NetworkPolicy;
use agentsfleetd::serve::Booted;

use crate::e2e::unique_ids;
use crate::e2e_seed::enrolment;

/// The native setup corpus, from this crate: the same files every corpus suite reads.
const CORPUS: &str = "../../../tests/fixtures/fleets";

/// The file a corpus fleet keeps its native setup in.
const DEFINITION_FILE: &str = "definition.json";

/// The connector whose declared credential mints, and so needs a grant.
const GITHUB_SERVICE: &str = "github";

/// The status a person's approval leaves on a grant.
const GRANT_APPROVED: &str = "approved";

/// Installs the corpus fleet `fleet_name` as the native rows a reviewed Save
/// writes.
pub(super) async fn install(
    booted: &Booted,
    workspace: &Uuid7,
    tenant: &str,
    fleet_name: &str,
    now: UnixMillis,
) -> Uuid7 {
    let definition = read_definition(fleet_name);
    let fleet = Uuid7::parse(&unique_ids().0).expect("a minted fleet id");
    let at = now.as_millis();
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    insert_fleet_row(&mut connection, &definition, &fleet, workspace, tenant, at).await;
    crate::e2e_native::select(&mut connection, fleet.as_str(), &definition, at).await;
    if definition
        .runtime()
        .credentials()
        .iter()
        .any(|name| name.as_str() == GITHUB_SERVICE)
    {
        approve(&mut connection, &fleet, at).await;
    }
    afd_dragonfly::FleetStreams::new(booted.queue.clone())
        .ensure_group(fleet.as_str())
        .await
        .expect("the fleet's stream and group exist");
    fleet
}

/// The corpus fleet's native setup, parsed.
fn read_definition(fleet_name: &str) -> Definition {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(CORPUS)
        .join(fleet_name)
        .join(DEFINITION_FILE);
    let text = std::fs::read_to_string(path).expect("the corpus setup is readable");
    Definition::parse(&text).expect("the corpus setup is a valid Definition")
}

/// The fleet row a Save writes: the runtime's name and its config.
async fn insert_fleet_row(
    connection: &mut sqlx::PgConnection,
    definition: &Definition,
    fleet: &Uuid7,
    workspace: &Uuid7,
    tenant: &str,
    at: i64,
) {
    let config = definition.config().value().to_string();
    FleetSeed::new(
        fleet.as_str(),
        workspace.as_str(),
        tenant,
        FLEET_STATUS_ACTIVE,
    )
    .name(definition.runtime().name().as_str())
    .config(&config)
    .at(at)
    .insert(&mut *connection)
    .await
    .expect("the corpus fleet's row inserts");
}

/// The GitHub grant, as a person's approval on the setup check leaves it.
async fn approve(connection: &mut sqlx::PgConnection, fleet: &Uuid7, at: i64) {
    sqlx::query(
        "INSERT INTO core.integration_grants
           (id, fleet_id, service, status, requested_reason, approved_at, created_at)
         VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $6)",
    )
    .bind(unique_ids().0)
    .bind(fleet.as_str())
    .bind(GITHUB_SERVICE)
    .bind(GRANT_APPROVED)
    .bind(afd_approval::Origin::Install.reason())
    .bind(at)
    .execute(connection)
    .await
    .expect("the approved grant inserts");
}

/// Enrols a runner labelled with every tag the installed fleet's row requires —
/// a fleet is placed only on a runner carrying its `required_tags` — and with
/// no egress control, since this runner reports none and the daemon withholds
/// leases from a host that cannot enforce what its policy demands. Then points
/// the fleet's affinity at it, so a ready mark another suite left behind cannot
/// take its place.
pub(super) async fn enrol(
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
