use super::Analytics;
use super::telemetry::Telemetry;

#[tokio::test]
async fn silent_analytics_is_debuggable_and_never_reports() {
    let analytics = Analytics::silent();
    assert!(!analytics.is_reporting());
    assert_eq!(format!("{analytics:?}"), "Analytics(false)");
    analytics.report(&Telemetry::ServerStarted { port: 8080 });
    analytics.flush().await;
}

#[tokio::test]
async fn configured_analytics_queues_without_blocking_the_caller() {
    for host in [None, Some("http://127.0.0.1:9")] {
        let analytics = Analytics::resolve("test-project", host).await;
        assert!(analytics.is_reporting());
        assert_eq!(format!("{analytics:?}"), "Analytics(true)");
        analytics.report(&Telemetry::ServerStarted { port: 8080 });
    }
}
