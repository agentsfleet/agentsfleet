//! Native boot and machine enrollment for the shared authoring scenarios.

use super::super::{decision::Boundary, provider::Provider};
use af_core::{clock, env::MapEnv, id::Uuid7};
use af_crypto::entropy::Entropy;
use afd_runner::Runners;
use afr_supervisor::{
    config::{self, Config},
    test_util::Session,
};
use agentsfleetd::{
    serve::{Booted, boot_with_authoring_boundary},
    supervisor::Supervisor,
};

pub(super) async fn connect(booted: &Booted) -> (Uuid7, String, Config, Session) {
    let registered = Runners::new(booted.database.clone(), Entropy::new())
        .register(&crate::e2e_seed::enrolment(), clock::now())
        .await
        .expect("register native runner");
    let token = registered.token.expose().to_owned();
    let config = native_config(booted, &token);
    let session = Session::connect(&config).await.expect("native connection");
    session.capacity(1);
    let bytes = serde_json::to_vec(&crate::wire::capable_beat()).expect("capability JSON");
    let beat = serde_json::from_slice(&bytes).expect("capability report");
    session.heartbeat(&beat).await.expect("native heartbeat");
    (registered.runner_id, token, config, session)
}

fn native_config(booted: &Booted, token: &str) -> Config {
    let api = format!("http://{}", booted.address);
    let origin = format!("https://localhost:{}", booted.runner_stream_address.port());
    let root = crate::support::runner_stream::root_file()
        .to_string_lossy()
        .into_owned();
    Config::from_env(&MapEnv::from_pairs([
        (config::ENV_API_URL, api.as_str()),
        (config::ENV_RUNNER_TOKEN, token),
        (config::ENV_STREAM_URL, origin.as_str()),
        (config::ENV_STREAM_CA, root.as_str()),
    ]))
    .expect("native runner configuration")
}

pub(super) async fn boot(
    supervisor: &mut Supervisor,
    blocked: bool,
    boundary: Option<&Boundary>,
) -> (Booted, Provider) {
    let provider = Provider::open(blocked).await;
    let transport = afd_authoring::Network::new()
        .expect("guarded client")
        .with_test_endpoint(provider.endpoint.clone())
        .expect("provider boundary");
    let transport = match boundary {
        Some(boundary) => transport.with_test_decision_client(boundary.client.clone()),
        None => transport,
    };
    let extra: Vec<_> = boundary
        .map(|boundary| ("PLATFORM_ADMIN_WORKSPACE_ID", boundary.workspace.as_str()))
        .into_iter()
        .collect();
    let env = crate::e2e::daemon_environment(
        &crate::e2e::lane(crate::e2e::DATABASE_LANE_KNOB),
        None,
        &extra,
    );
    let booted = boot_with_authoring_boundary(&env, 0, supervisor, transport)
        .await
        .expect("daemon boot");
    (booted, provider)
}
