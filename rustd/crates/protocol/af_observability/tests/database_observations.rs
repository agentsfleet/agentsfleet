//! Actual SDK output distinguishes measured zero, missing data and each wait outcome.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use af_observability::metrics::instrument::Reading;
use af_observability::metrics::label::database::{AcquireOutcome, PoolRole};
use af_observability::producers::{GaugeSources, database};
use af_observability::test_util::Capture;
use opentelemetry::KeyValue;

#[test]
fn should_export_pool_zeros_and_wait_outcomes() {
    database::acquired(PoolRole::Api, AcquireOutcome::Acquired, Duration::ZERO);
    let present = Arc::new(AtomicBool::new(true));
    let available = Arc::clone(&present);
    let mut sources = GaugeSources::silent();
    sources.database_pools = Arc::new(move || {
        if available.load(Ordering::Relaxed) {
            vec![Reading {
                value: 0,
                attributes: vec![
                    KeyValue::new("role", "api"),
                    KeyValue::new("state", "waiting"),
                ],
            }]
        } else {
            Vec::new()
        }
    });
    let capture = Capture::install_with_sources(&sources);
    assert_eq!(
        capture.gauge(
            "agentsfleet_db_pool_connections",
            &[("role", "api"), ("state", "waiting")]
        ),
        Some(0)
    );
    for role in PoolRole::ALL {
        for outcome in AcquireOutcome::ALL {
            let labels = [("role", role.as_str()), ("outcome", outcome.as_str())];
            assert_eq!(
                capture.histogram_count("agentsfleet_db_pool_acquire_duration_seconds", &labels),
                0
            );
            database::acquired(*role, *outcome, Duration::from_millis(17));
            assert_eq!(
                capture.histogram_count("agentsfleet_db_pool_acquire_duration_seconds", &labels),
                1
            );
            assert!(
                (capture.histogram_sum("agentsfleet_db_pool_acquire_duration_seconds", &labels)
                    - 0.017)
                    .abs()
                    < f64::EPSILON
            );
        }
    }
    present.store(false, Ordering::Relaxed);
    assert_eq!(
        capture.gauge(
            "agentsfleet_db_pool_connections",
            &[("role", "api"), ("state", "waiting")]
        ),
        None
    );
    let _same_provider = Capture::install_with_sources(&sources);
}
