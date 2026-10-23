//! Live and unavailable Dragonfly handles, plus deterministic ledger fixtures.
//!
//! Shared-database tests scope rows by fleet. Tests asserting deployment-wide
//! candidate or backlog counts use a private database.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_core::env::{EnvSource as _, ProcessEnv};
use afd_dragonfly::{Dragonfly, DragonflyConfig, DragonflyRole};

/// The lane's Dragonfly URL.
const URL_KNOB: &str = "TEST_DRAGONFLY_URL";

/// The lane's Dragonfly certificate authority, when it serves TLS.
const CA_KNOB: &str = "TEST_DRAGONFLY_CA_CERT";

/// The configuration the lane hands these suites.
pub(crate) fn config() -> DragonflyConfig {
    let url = ProcessEnv.get(URL_KNOB).unwrap_or_else(|| {
        panic!("{URL_KNOB} is unset — run these through `make test-integration-rustd`")
    });
    DragonflyConfig::from_url(DragonflyRole::Default, url)
        .with_ca_cert_file(ProcessEnv.get(CA_KNOB).map(Into::into))
}

/// A Dragonfly nobody is listening on.
///
/// Port 1 is reserved and unbound on every platform this builds for, so a
/// command fails on connection refusal rather than waiting out a timeout — the
/// difference between a suite that runs in milliseconds and one that runs in
/// request budgets. Plain `redis://` rather than the lane's `rediss://`: no
/// socket is ever opened, so a certificate authority would be configuration
/// nothing reads.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// A handle over a Dragonfly that will not answer, for the drop paths.
///
/// `Dragonfly::unreachable` skips the ping `connect` performs, which is the only
/// way to hold this: the lane's Dragonfly is SHARED by every test binary running in
/// parallel, so pausing the container or killing the server would fail
/// unrelated suites at the same instant. A handle one test owns fails only that
/// test's commands.
pub(crate) fn unreachable() -> Dragonfly {
    Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket and cannot fail")
}

/// Connects to the lane's Dragonfly.
pub(crate) async fn connect() -> Dragonfly {
    afd_dragonfly::test_util::connect_live(&config())
        .await
        .expect("the lane's Dragonfly must be reachable")
}

/// Place a durable admission with the fixture's deterministic event clock.
pub(crate) async fn enqueue(
    database: &afd_db::Db,
    fleet: &str,
    workspace: &str,
    actor: &str,
    event_type: &str,
    request_json: &str,
    created_at: jiff::Timestamp,
) -> String {
    use jiff_sqlx::ToSqlx as _;
    let event = afd_db::test_util::mint_id();
    sqlx::query("INSERT INTO fleet.admissions (id,fleet_id,workspace_id,producer,producer_key,payload_digest,actor,event_type,request_json,event_created_at,created_at,updated_at) VALUES ($1::uuid,$2::uuid,$3::uuid,'steer',$1,'fixture',$4,$5,$6,$7,$7,$7)")
        .bind(&event).bind(fleet).bind(workspace).bind(actor).bind(event_type)
        .bind(request_json).bind(created_at.to_sqlx())
        .execute(&mut *database.acquire().await.expect("runtime connection"))
        .await.expect("durable admission");
    event
}
