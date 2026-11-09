//! Optional boot surfaces that degrade without refusing the daemon.

use af_observability::Analytics;
use afd_sse::{Live, StreamLimit};

use crate::identity::Capabilities;

/// Says which identity surfaces this instance can actually serve.
pub(super) fn announce_identity(capabilities: &Capabilities) {
    match capabilities {
        Capabilities::Provider(_built) => {
            tracing::info!(
                event = "identity_provider_configured",
                "identity provider configured — tenant and runner planes both serve"
            );
        }
        Capabilities::Unconfigured(_absent) => {
            let code = af_core::error_code::AUTH_UNAVAILABLE.as_str();
            tracing::warn!(
                error_code = code,
                event = "identity_provider_unusable",
                "identity provider unusable — the runner plane serves normally \
                 and every tenant-plane capability read answers unavailable"
            );
        }
    }
}

/// The live-stream surface over the bus boot already connected.
///
/// Nothing degrades here any more: an unreachable bus refused boot as
/// `UZ-STARTUP-004` before the router existed (M244 §2).
pub(super) fn live_over(bus: &afd_bus::Handle, max_streams: usize) -> Live {
    Live::new(bus.clone(), StreamLimit::new(max_streams))
}

/// The product-analytics reporter, or its silent form.
pub(super) async fn open_analytics(config: Option<&crate::preflight::PostHogConfig>) -> Analytics {
    match config {
        Some(project) => Analytics::resolve(&project.project_key, project.host.as_deref()).await,
        None => Analytics::silent(),
    }
}

#[cfg(test)]
mod tests {
    use afd_auth::capability::NoCapabilitySource;

    use super::{announce_identity, live_over, open_analytics};
    use crate::identity::Capabilities;
    use crate::preflight::PostHogConfig;

    #[test]
    fn an_unconfigured_identity_is_announced_as_a_reduced_surface() {
        afd_db::test_util::install_subscriber();
        announce_identity(&Capabilities::Unconfigured(NoCapabilitySource));
    }

    #[tokio::test]
    async fn the_live_surface_rides_the_booted_bus_at_its_capacity() -> afd_bus::Result<()> {
        let (bus, _server) = afd_bus::test_util::detached()?;
        let live = live_over(&bus, 3);
        assert!(live.bus().is_some());
        assert_eq!(live.capacity(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn analytics_configuration_selects_reporting_or_silence() {
        assert!(!open_analytics(None).await.is_reporting());
        let configured = PostHogConfig {
            project_key: "ph_fixture".into(),
            host: Some("https://events.example.test".into()),
        };
        let reporting = open_analytics(Some(&configured)).await;
        assert!(reporting.is_reporting());
        reporting.flush().await;
    }
}
