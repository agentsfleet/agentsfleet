//! A §6 scenario: a first-party fleet's native setup saved into a booted daemon.
//!
//! Where `e2e::scenario` seeds a fleet with an empty tool list, this reads the
//! corpus fleet's `definition.json` — the native Definition a reviewed Save
//! stores — so the policy every lease compiles from it is that fleet's own. The GitHub grant lands approved, which is what a
//! person's answer on the setup's grant check leaves behind. The credentials
//! the definition declares are sealed into the vault first, as setup readiness
//! requires.
//!
//! The daemon boots through `boot_with_exchanger`, so a GitHub mint runs the
//! grant check, the vault read and the broker, and only the token endpoint is
//! this file's [`MintedToken`]. The result is an ordinary [`Scenario`]: every
//! reader in `e2e_reads` and every tenant helper works on it unchanged.
//!
//! The rows a scenario stands on live in [`seed`]; the installed fleet and the
//! runner that leases it live in [`fleet`].
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

mod fleet;
mod seed;

use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_credential::credential::{Ask, Exchanger, Minted, Outcome};
use afd_credential::secrets::connector::Connector;
use afd_wire::event::EventType;
use agentsfleetd::serve::{Booted, boot_with_exchanger};
use agentsfleetd::supervisor::Supervisor;
use jiff::Timestamp;
use serde_json::Value;

use self::fleet::{enrol, install};
pub(crate) use self::seed::seal;
use self::seed::{fresh_installation, github_handle, seed_install, seed_workspace};
use crate::e2e::{
    DATABASE_LANE_KNOB, EPHEMERAL, READY_STREAM, Scenario, daemon_environment, lane, unique_ids,
};
use crate::e2e_db::scenario_database;
use crate::e2e_event::enqueue;
use crate::e2e_seed::{DEEP_POOL, seed_model_rate, seed_platform_default, seed_wallet};
use crate::e2e_seed_keys::seed_provider_key;
use crate::support::install_subscriber;

/// The token every GitHub mint answers with. A suite asserts it reached the
/// upstream in `Authorization` and nowhere the model could read it.
pub(crate) const MINTED_TOKEN: &str = "ghs_fixture_minted_installation_token";

/// How long a minted token lives: an hour, as GitHub's installation tokens do.
const TOKEN_LIFETIME: std::time::Duration = std::time::Duration::from_hours(1);

/// GitHub's token endpoint, as the lane stands it in.
#[derive(Debug)]
pub(crate) struct MintedToken;

impl Exchanger for MintedToken {
    fn exchange<'a>(
        &'a self,
        _connector: &'a dyn Connector,
        _ask: Ask<'a>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Outcome> + Send + 'a>> {
        let expires_at = afd_core::clock::after(afd_core::clock::now(), TOKEN_LIFETIME);
        Box::pin(std::future::ready(Outcome::Ok(Minted {
            token: MINTED_TOKEN.to_owned().into(),
            expires_at,
            rotated_refresh_token: None,
        })))
    }
}

/// One static credential a definition declares: its vault name and its fields.
pub(crate) struct Secret {
    pub(crate) name: &'static str,
    pub(crate) body: Value,
}

/// Boots a daemon, installs the corpus fleet named `fleet_name` with `secrets`
/// and the GitHub handle sealed beside them, enrols a runner, and puts one
/// steer on its stream.
///
/// A steer for every fleet, webhook-triggered ones included: the pull path
/// leases any event type it can name, and the scripted model, not the event
/// body, decides what each run reads. `provider_base` points the identity
/// provider at a suite's listener, for one that reads through tenant routes.
pub(crate) async fn install_definition(
    supervisor: &mut Supervisor,
    fleet_name: &str,
    secrets: &[Secret],
    provider_base: Option<&str>,
) -> Scenario {
    install_subscriber();
    let exclusive = READY_STREAM.lock().await;
    let booted = boot_minting(supervisor, provider_base).await;
    let now = afd_core::clock::now();
    let (_unused, workspace, tenant) = unique_ids();
    seed_workspace(&booted, &workspace, &tenant, now).await;
    seed_wallet(&booted, &tenant, DEEP_POOL, now).await;
    seed_model_rate(&booted, now).await;
    let default = seed_platform_default(&booted, &workspace, now).await;
    seed_provider_key(&booted, &workspace, now).await;
    let workspace_id = Uuid7::parse(&workspace).expect("a minted workspace id");
    seal_credentials(&booted, &workspace, &workspace_id, secrets, now).await;
    let fleet = install(&booted, &workspace_id, &tenant, fleet_name, now).await;
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

/// Boots the daemon on the lane's database with [`MintedToken`] standing in
/// for GitHub's token endpoint.
async fn boot_minting(supervisor: &mut Supervisor, provider_base: Option<&str>) -> Booted {
    let database_url = scenario_database(&lane(DATABASE_LANE_KNOB));
    boot_with_exchanger(
        &daemon_environment(&database_url, provider_base, &[]),
        EPHEMERAL,
        supervisor,
        Arc::new(MintedToken),
    )
    .await
    .expect("the lane's Postgres and Dragonfly are up")
}

/// Seals the GitHub handle for a fresh installation, lands that
/// installation's routing row, then seals every credential the definition
/// declares.
async fn seal_credentials(
    booted: &Booted,
    workspace: &str,
    workspace_id: &Uuid7,
    secrets: &[Secret],
    now: Timestamp,
) {
    let installation = fresh_installation();
    seal(
        booted,
        workspace_id,
        "github",
        &github_handle(&installation),
        now,
    )
    .await;
    seed_install(booted, workspace, &installation, now).await;
    for secret in secrets {
        seal(
            booted,
            workspace_id,
            secret.name,
            &secret.body.to_string(),
            now,
        )
        .await;
    }
}
