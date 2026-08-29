//! Optional boot surfaces that degrade without refusing the daemon.

use afd_observability::Analytics;
use afd_redis::{RedisConfig, SubscriptionHub};
use afd_sse::{Ceiling, Live};

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
            let code = afd_core::error_code::AUTH_UNAVAILABLE.as_str();
            tracing::warn!(
                error_code = code,
                event = "identity_provider_unusable",
                "identity provider unusable — the runner plane serves normally \
                 and every tenant-plane capability read answers unavailable"
            );
        }
    }
}

/// The live-stream surface, or its silent form when the hub will not open.
pub(super) async fn open_live(config: &RedisConfig, max_streams: usize) -> Live {
    let ceiling = Ceiling::new(max_streams);
    match SubscriptionHub::start(config.clone()).await {
        Ok(hub) => Live::new(hub, ceiling),
        Err(unopened) => {
            let code = afd_core::error_code::STARTUP_REDIS_CONNECT.as_str();
            let reason = unopened.to_string();
            tracing::warn!(
                error_code = code,
                reason,
                event = "hub_unavailable",
                "the live-stream surface will carry no frames; every other verb is unaffected"
            );
            Live::detached(ceiling)
        }
    }
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
    use std::time::Duration;

    use afd_auth::capability::NoCapabilitySource;
    use afd_redis::config::RedisRole;

    use super::{announce_identity, open_analytics, open_live};
    use crate::identity::Capabilities;
    use crate::preflight::PostHogConfig;

    #[test]
    fn an_unconfigured_identity_is_announced_as_a_reduced_surface() {
        afd_db::test_util::install_subscriber();
        announce_identity(&Capabilities::Unconfigured(NoCapabilitySource));
    }

    #[tokio::test]
    async fn a_failed_hub_becomes_a_capacity_bounded_silent_surface() {
        afd_db::test_util::install_subscriber();
        let config =
            afd_redis::RedisConfig::from_url(RedisRole::Api, "redis://127.0.0.1:1".to_owned())
                .with_connect_timeout(Duration::from_millis(25));

        let live = open_live(&config, 3).await;
        assert!(live.hub().is_none());
        assert_eq!(live.capacity(), 3);
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
