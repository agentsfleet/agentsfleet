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

/// A recording reporter keeps each event, in order, and counts as reporting.
#[cfg(feature = "test-util")]
#[tokio::test]
async fn recording_analytics_keeps_every_event_in_order() {
    let (analytics, recorded) = Analytics::recording();
    assert!(analytics.is_reporting());
    let started = Telemetry::ServerStarted { port: 8080 };
    let again = Telemetry::ServerStarted { port: 8081 };
    analytics.report(&started);
    analytics.clone().report(&again);
    analytics.flush().await;
    assert_eq!(recorded.events(), vec![started, again]);
}

/// A recording whose lock a panicking thread poisoned still keeps the next
/// event and still reads back every one: a suite that already failed once
/// must not then read an empty list and fail a second time, misleadingly.
#[cfg(feature = "test-util")]
#[tokio::test]
async fn should_keep_and_read_events_when_lock_poisoned() {
    let (analytics, recorded) = Analytics::recording();
    let before = Telemetry::ServerStarted { port: 8080 };
    let after = Telemetry::ServerStarted { port: 8081 };
    analytics.report(&before);
    let shared = recorded.clone();
    let poisoned = std::thread::spawn(move || {
        let _held = shared.0.lock();
        std::panic::resume_unwind(Box::new("a suite thread panics holding the lock"));
    })
    .join();
    assert!(poisoned.is_err(), "the thread panicked");
    assert!(recorded.0.is_poisoned());
    analytics.report(&after);
    assert_eq!(recorded.events(), vec![before, after]);
}
