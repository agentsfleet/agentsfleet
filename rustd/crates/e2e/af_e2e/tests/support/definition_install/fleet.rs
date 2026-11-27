//! The installed fleet: its row, the access a person approved for it, its
//! Slack channel when a scenario names one, and the runner enrolled to run it.
//!
//! A fleet holds no setup; what a corpus scenario needs is the grants a person
//! approved and the network it may reach, so each corpus name maps to those.

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_wire::runner::{NetworkPolicy, SandboxTier};
use af_wire::trigger_verb::TriggerKind;
use afd_db::test_util::{FleetSeed, GrantSeed};
use afd_runner::Runners;
use afd_setup::trigger::NewTrigger;
use afd_state::sql::FLEET_STATUS_ACTIVE;
use agentsfleetd::serve::Booted;
use jiff::Timestamp;
use serde_json::json;

use crate::e2e::unique_ids;
use crate::e2e_seed::{enrolment, scenario_tag};

/// The connector whose grant mints, and so carries a repository scope.
const GITHUB_SERVICE: &str = "github";

/// The status a person's approval leaves on a grant.
const GRANT_APPROVED: &str = "approved";

/// The kinds a grant's channel is stored as.
const KIND_SECRET: &str = afd_approval::GrantKind::Secret.as_str();
const KIND_INTEGRATION: &str = afd_approval::GrantKind::Integration.as_str();

/// What a corpus scenario's fleet holds: the GitHub reach a person approved,
/// the workspace secrets approved beside it, and whether its network is open.
struct Holding {
    name: &'static str,
    repository: &'static str,
    access: &'static str,
    base: Option<&'static str>,
    secrets: &'static [&'static str],
    public: bool,
}

/// The holding each corpus fleet runs with.
const HOLDINGS: [Holding; 7] = [
    held(
        "ci-repairer",
        "agentsfleet/linkwarden",
        "write",
        Some("dev"),
        &[],
        true,
    ),
    held(
        "ci-responder",
        "agentsfleet/linkwarden",
        "read",
        None,
        &[],
        true,
    ),
    held(
        "delegating-triager",
        "agentsfleet/greeter",
        "read",
        None,
        &[],
        false,
    ),
    held(
        "github-log-reader",
        "agentsfleet/linkwarden",
        "read",
        None,
        &[],
        true,
    ),
    held(
        "github-pr-reviewer",
        "agentsfleet/linkwarden",
        "write",
        Some("main"),
        &[],
        false,
    ),
    held(
        "incident-repairer",
        "agentsfleet/agentsfleet",
        "write",
        Some("main"),
        &["elastic", "grafana"],
        true,
    ),
    held(
        "test-fixer",
        "agentsfleet/greeter",
        "read",
        None,
        &[],
        false,
    ),
];

/// One row of [`HOLDINGS`].
const fn held(
    name: &'static str,
    repository: &'static str,
    access: &'static str,
    base: Option<&'static str>,
    secrets: &'static [&'static str],
    public: bool,
) -> Holding {
    Holding {
        name,
        repository,
        access,
        base,
        secrets,
        public,
    }
}

/// The holding of the corpus fleet `fleet_name`.
fn holding(fleet_name: &str) -> &'static Holding {
    HOLDINGS
        .iter()
        .find(|holding| holding.name == fleet_name)
        .expect("a corpus fleet the table names")
}

/// Installs the corpus fleet `fleet_name` as the rows a first message and a
/// person's approvals leave.
pub(super) async fn install(
    booted: &Booted,
    workspace: &Uuid7,
    tenant: &Uuid7,
    fleet_name: &str,
    now: Timestamp,
    channel: Option<&str>,
) -> Uuid7 {
    let holding = holding(fleet_name);
    let fleet = unique_ids().0;
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    insert_fleet_row(
        &mut connection,
        fleet_name,
        holding,
        &fleet,
        workspace,
        tenant,
        now,
    )
    .await;
    approve(&mut connection, &fleet, holding, now).await;
    if let Some(channel) = channel {
        subscribe(&mut connection, &fleet, workspace, channel).await;
    }
    fleet
}

/// The fleet row a first message writes, named for the scenario.
///
/// Placed on its own scenario tag, so only the runner [`enrol`] labels for it
/// is handed its work. The shared unplaced tag would not do: `enrol` copies
/// the row's tags onto the runner, and a runner carrying the tag every
/// unplaced fixture fleet requires is assigned the work other suites left on
/// them.
async fn insert_fleet_row(
    connection: &mut sqlx::PgConnection,
    fleet_name: &str,
    holding: &Holding,
    fleet: &Uuid7,
    workspace: &Uuid7,
    tenant: &Uuid7,
    at: Timestamp,
) {
    let tag = scenario_tag(fleet);
    let egress = if holding.public {
        "public"
    } else {
        "restricted"
    };
    FleetSeed::new(fleet, workspace, tenant, FLEET_STATUS_ACTIVE)
        .name(fleet_name)
        .at(at)
        .tags(&[tag.as_str()])
        .network(egress, false)
        .insert(&mut *connection)
        .await
        .expect("the corpus fleet's row inserts");
}

/// The grants a person's approvals leave: GitHub at the scenario's scope, and
/// each workspace secret it reads.
async fn approve(
    connection: &mut sqlx::PgConnection,
    fleet: &Uuid7,
    holding: &Holding,
    at: Timestamp,
) {
    // A read scope names no base, since it opens no Pull Request.
    let (repositories, access) = ([holding.repository], holding.access);
    let scope = holding
        .base
        .map_or_else(
            || json!({"repositories": repositories, "access": access}),
            |base| json!({"repositories": repositories, "access": access, "base": base}),
        )
        .to_string();
    GrantSeed::new(fleet, GITHUB_SERVICE, KIND_INTEGRATION, GRANT_APPROVED)
        .scope(&scope)
        .approved(at)
        .at(at)
        .insert(&mut *connection)
        .await
        .expect("the approved GitHub grant inserts");
    for name in holding.secrets {
        GrantSeed::new(fleet, name, KIND_SECRET, GRANT_APPROVED)
            .approved(at)
            .at(at)
            .insert(&mut *connection)
            .await
            .expect("the approved secret grant inserts");
    }
}

/// The `mention` row a fleet's `trigger_add` writes for `channel`.
async fn subscribe(
    connection: &mut sqlx::PgConnection,
    fleet: &Uuid7,
    workspace: &Uuid7,
    channel: &str,
) {
    let id = unique_ids().0;
    afd_setup::trigger::insert(
        connection,
        NewTrigger {
            id: &id,
            fleet,
            workspace,
            trigger_type: TriggerKind::Mention,
            source: "slack",
            events: None,
            repositories: None,
            verifier_scheme: None,
            verifier_header: None,
            secret_name: None,
            channel: Some(channel),
        },
    )
    .await
    .expect("the mention row inserts")
    .expect("the fleet held no mention row");
}

/// Enrols a runner labelled with every tag the installed fleet's row requires —
/// a fleet is placed only on a runner carrying its `required_tags` — as
/// `dev_none` with no egress control, since this runner runs the unsandboxed
/// engine and the daemon withholds runs from a host whose engine is not its
/// tier's. Then points
/// the fleet's affinity at it, so a ready mark another suite left behind cannot
/// take its place.
pub(super) async fn enrol(booted: &Booted, fleet: &Uuid7, now: Timestamp) -> (Uuid7, String) {
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let required: Vec<String> =
        sqlx::query_scalar("SELECT required_tags FROM fleet.fleets WHERE id = $1")
            .bind(fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("the installed fleet's tags read");
    drop(connection);
    let mut request = enrolment();
    request.assigned_policy.sandbox_tier = SandboxTier::DevNone;
    request.assigned_policy.network_policy = NetworkPolicy::AllowAll;
    request.labels.extend(required.into_iter().map(Cow::Owned));
    let enrolled = Runners::new(booted.database.clone(), Entropy::new())
        .register(&request, "fixture:platform-operator", now)
        .await
        .expect("enrolment must succeed");
    afd_fleet::run::Runs::new(booted.database.clone(), booted.bus.clone(), Entropy::new())
        .assign(fleet, &enrolled.runner_id, std::time::Duration::ZERO)
        .await
        .expect("the thread assignment runs")
        .expect("a freshly installed fleet is unassigned");
    (enrolled.runner_id, enrolled.token.expose().to_owned())
}
