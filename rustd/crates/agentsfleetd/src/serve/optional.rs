//! Optional boot surfaces that degrade without refusing the daemon.

use afd_core::error_code::{Coded as _, Logged};
use afd_dragonfly::{DragonflyConfig, SubscriptionHub};
use afd_observability::Analytics;
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
pub(super) async fn open_live(config: &DragonflyConfig, max_streams: usize) -> Live {
    let ceiling = Ceiling::new(max_streams);
    match SubscriptionHub::start(config.clone()).await {
        Ok(hub) => Live::new(hub, ceiling),
        Err(unopened) => {
            let code = afd_core::error_code::STARTUP_DRAGONFLY_CONNECT.as_str();
            let Logged {
                error_code: cause_error_code,
                reason,
            } = unopened.logged();
            tracing::warn!(
                error_code = code,
                cause_error_code,
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
    use afd_dragonfly::config::DragonflyRole;

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
        let config = afd_dragonfly::DragonflyConfig::from_url(
            DragonflyRole::Api,
            "redis://127.0.0.1:1".to_owned(),
        )
        .with_connect_timeout(Duration::from_millis(25));

        let live = open_live(&config, 3).await;
        assert!(live.hub().is_none());
        assert_eq!(live.capacity(), 3);
    }

    #[tokio::test]
    async fn an_unopened_hub_reason_carries_no_code_prefix() {
        let config = afd_dragonfly::DragonflyConfig::from_url(
            DragonflyRole::Api,
            "redis://127.0.0.1:1".to_owned(),
        )
        .with_connect_timeout(Duration::from_millis(25));
        let log = afd_core::test_util::trace::Capture::install();

        drop(open_live(&config, 3).await);

        let line = log.only("hub_unavailable");
        let reason = line.field("reason").unwrap_or_default();
        assert_eq!(
            line.field("error_code"),
            Some(afd_core::error_code::STARTUP_DRAGONFLY_CONNECT.as_str())
        );
        assert_eq!(
            line.field("cause_error_code"),
            Some(afd_core::error_code::STARTUP_DRAGONFLY_CONNECT.as_str()),
            "the unopened hub's own code rides `cause_error_code`"
        );
        assert!(
            !reason.is_empty() && !reason.contains("[UZ-"),
            "the code is the line's `error_code`, never the reason's prefix: {reason}"
        );
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
