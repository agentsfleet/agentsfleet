//! Explicit vendor boundaries for daemon integration tests.

use std::sync::Arc;

use afd_core::env::EnvSource;
use afd_credential::credential::Exchanger;

use super::{BootFailure, Booted, open, open_analytics, preflight};
use crate::supervisor::Supervisor;

/// Boots real admission and routing while connector calls stay on a local vendor.
///
/// # Errors
/// As [`super::boot_with_exchanger`]. This exists only with `test-util`.
pub async fn boot_with_connector_boundary<E: EnvSource + ?Sized>(
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
