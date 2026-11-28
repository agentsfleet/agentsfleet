//! Explicit vendor boundaries for daemon integration tests.

use std::sync::Arc;

use af_core::env::EnvSource;
use afd_credential::credential::Exchanger;

use super::{BootFailure, Booted, open, open_analytics, preflight};
use crate::supervisor::Supervisor;

/// Boots real admission and routing while connector calls stay on a local vendor.
///
/// # Errors
/// As [`super::boot_with_exchanger`]. This exists only with `test-util`.
pub async fn boot_with_connector_boundary<E: EnvSource + Sync + ?Sized>(
    env: &E,
    port: u16,
    supervisor: &mut Supervisor,
    exchanger: Arc<dyn Exchanger>,
    endpoint: String,
) -> Result<Booted, BootFailure> {
    let config = preflight(env)?
        .with_exchanger(exchanger)
        .with_connector_endpoint(endpoint);
    let analytics = open_analytics(config.posthog()).await;
    open(config, analytics, port, supervisor).await
}

/// Boots the real web verbs with the supplied vendor clients.
///
/// # Errors
/// As [`super::boot`].
pub async fn boot_with_web_clients<E: EnvSource + Sync + ?Sized>(
    env: &E,
    port: u16,
    supervisor: &mut Supervisor,
    clients: (af_providers::web::Parallel, af_providers::web::Exa),
) -> Result<Booted, BootFailure> {
    let mut config = preflight(env)?;
    config.web_clients = Some(Box::new(clients));
    let analytics = open_analytics(config.posthog()).await;
    open(config, analytics, port, supervisor).await
}
