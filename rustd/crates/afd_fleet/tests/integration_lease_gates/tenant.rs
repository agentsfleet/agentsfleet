//! One lease reads its tenant once, and its meter reset rides the lease row.
//!
//! Counted off the statements sqlx logs as it runs them, under a subscriber
//! scoped to this test's thread: the suite runs leases in parallel, and a
//! server-side tally would count theirs too.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber, subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt as _};
use tracing_subscriber::registry::Registry;

use super::seed::seed_provider_resolution;
use super::*;

/// The target sqlx logs each executed statement under.
const STATEMENT_TARGET: &str = "sqlx::query";

/// The field that carries the statement's text.
const STATEMENT_FIELD: &str = "db.statement";

/// Tokens an earlier run left on the slot's cursor.
const METERED: i64 = 1_234;

/// Counts executions of the tenant lookup.
struct TenantReads(Arc<AtomicUsize>);

impl<S: Subscriber> Layer<S> for TenantReads {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() != STATEMENT_TARGET {
            return;
        }
        let mut statement = IsTenantRead(false);
        event.record(&mut statement);
        if statement.0 {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Whether one logged statement is the tenant lookup.
struct IsTenantRead(bool);

impl Visit for IsTenantRead {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == STATEMENT_FIELD
            && value.contains(afd_billing::sql::SELECT_TENANT_FOR_WORKSPACE)
        {
            self.0 = true;
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_issue_reads_the_tenant_once() {
    // The provider resolves against the payer and the money gates bill it, and
    // both used to read it; a second read could only disagree with the first.
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, READ_BOUND_CONFIG).await;
    let _default = seed_provider_resolution(&fixtures, &seeded.fleet).await;
    let claimed = claim(&fixtures, &seeded).await;
    fixtures.set_metered_input(&seeded.fleet, METERED).await;

    let reads = Arc::new(AtomicUsize::new(0));
    let answer = {
        let _scoped =
            subscriber::set_default(Registry::default().with(TenantReads(Arc::clone(&reads))));
        drive(&fixtures, &seeded, claimed).await
    };

    assert!(
        !answer.contains(NO_LEASE),
        "the lease was refused: {answer}"
    );
    assert_eq!(
        reads.load(Ordering::Relaxed),
        1,
        "one lease reads its tenant once"
    );
    assert_eq!(
        fixtures
            .affinity_column(&seeded.fleet, crate::lease_reads::COLUMN_METERED_INPUT)
            .await,
        Some("0".to_owned()),
        "the fresh lease's own insert reset the cursor an earlier run left"
    );

    fixtures.cleanup().await;
}
