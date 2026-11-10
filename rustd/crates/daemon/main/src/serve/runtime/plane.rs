//! Assemble request services after boot has opened their shared dependencies.

use super::{identity_webhook_secret, login_config, schedule_config};
use crate::{
    identity::{Capabilities, Sessions, SignupWriteback},
    plane::ServingPlane,
    preflight::BootConfig,
};
use af_crypto::secret::Kek;
use af_observability::Analytics;
use afd_db::Db;
use afd_sse::Live;
use std::sync::Arc;

pub(super) struct Inputs<'a> {
    pub(super) config: &'a BootConfig,
    pub(super) analytics: &'a Analytics,
    pub(super) database: &'a Db,
    pub(super) bus: &'a afd_bus::Handle,
    pub(super) kek: &'a Arc<Kek>,
    pub(super) identity: (Capabilities, Sessions, SignupWriteback),
    pub(super) broker: Arc<afd_credential::credential::Broker>,
    pub(super) live: Live,
    pub(super) authoring_transport: afd_authoring::Network,
    pub(super) vendor_client: reqwest::Client,
    pub(super) schedule_client: reqwest::Client,
}

pub(super) fn build(inputs: Inputs<'_>) -> ServingPlane {
    let Inputs {
        config,
        analytics,
        database,
        bus,
        kek,
        identity,
        broker,
        live,
        authoring_transport,
        vendor_client,
        schedule_client,
    } = inputs;
    ServingPlane::new(crate::plane::PlaneParts {
        authoring_transport,
        database: database.clone(),
        bus: bus.clone(),
        runner_wake: config.runner_wake().clone(),
        frame_memory: config.frame_memory(),
        kek: Arc::clone(kek),
        capabilities: identity.0,
        signup_writeback: identity.2,
        sessions: identity.1,
        platform_admin_workspace: config.platform_admin_workspace().cloned(),
        checkpoint_storage: config.checkpoint_storage().cloned(),
        identity_webhook_secret: identity_webhook_secret(config),
        broker,
        live,
        analytics: analytics.clone(),
        slack_api_base: config.slack_api_base().to_owned(),
        vendor_client,
        schedule: schedule_config(config, schedule_client),
        login: login_config(config),
    })
}
