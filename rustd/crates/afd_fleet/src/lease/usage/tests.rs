#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_billing::biller::Attribution;
use afd_billing::rates::Posture;
use afd_core::test_util::trace::Capture;
use afd_wire::usage::{DAEMON_SEQ_BASE, RequestUsage};
use tracing::Level;

use super::{EVENT_TOKENS_UNPRICED, Pricing, Slice, charged};
use crate::lease::test_dead;

/// One second of runtime.
const ONE_SECOND_MS: i64 = 1_000;

fn slice(runtime_ms: i64, daemon_lines: i64) -> Slice {
    Slice {
        daemon_lines,
        checked_at: test_dead::AT,
        started_at: test_dead::AT,
        runtime_ms,
        tenant_id: test_dead::id(2).as_str().to_owned(),
        workspace_id: test_dead::id(3).as_str().to_owned(),
        fleet_id: test_dead::id(1).as_str().to_owned(),
        fleet_name: None,
        event_id: "1767225600000-1".to_owned(),
    }
}

fn request(seq: u32) -> RequestUsage {
    RequestUsage {
        seq,
        at: test_dead::AT,
        input_tokens: 25_000,
        cached_input_tokens: 2_500,
        cache_write_tokens: 0,
        cache_write_5m_tokens: 0,
        cache_write_1h_tokens: 0,
        output_tokens: 4_000,
    }
}

async fn pricing(posture: Posture) -> Pricing {
    Pricing::load(
        &test_dead::database(),
        posture,
        "anthropic",
        "claude-opus-5-5",
        "lease-fixture",
    )
    .await
}

/// A catalogue that cannot be read still logs every request: its tokens are
/// counted unpriced on the line, and the warning names the lease.
#[tokio::test]
async fn an_unreadable_catalogue_logs_the_tokens_unpriced() {
    let log = Capture::install();
    let pricing = pricing(Posture::Platform).await;
    let settlement = pricing
        .settlement(&test_dead::id(4), slice(0, 0), &[request(0)])
        .expect("prices")
        .expect("a request logs a line");
    let line = settlement.lines.first().expect("one line");
    assert_eq!(line.unpriced_tokens, 31_500, "every token is visible");
    assert_eq!(line.price, None);
    let event = log.only(EVENT_TOKENS_UNPRICED);
    assert_eq!(event.level, Level::WARN);
    assert_eq!(event.field("lease_id"), Some("lease-fixture"));
}

/// Each request is its own line under the runner's number, and the measured
/// time is one more line in the daemon's range, after the ones it already
/// logged; a self-managed run pays the run fee only.
#[tokio::test]
async fn requests_keep_their_numbers_and_runtime_takes_the_daemons() {
    let pricing = pricing(Posture::SelfManaged).await;
    let settlement = pricing
        .settlement(&test_dead::id(4), slice(ONE_SECOND_MS, 2), &[request(0), request(1)])
        .expect("prices")
        .expect("lines");
    let numbers: Vec<u32> = settlement.lines.iter().map(|line| line.usage.seq).collect();
    assert_eq!(numbers, [0, 1, DAEMON_SEQ_BASE + 2]);
    assert_eq!(
        charged(Some(&settlement)).as_i64(),
        afd_billing::RUN_NANOS_PER_SEC,
        "one second of runtime is the whole self-managed charge"
    );
}

/// A line of a purged fleet still logs, naming the fleet by its identifier:
/// the line is money owed, and a missing name must never cost it.
#[tokio::test]
async fn a_slice_of_a_purged_fleet_names_it_by_its_identifier() {
    let pricing = pricing(Posture::SelfManaged).await;
    let settlement = pricing
        .settlement(&test_dead::id(4), slice(ONE_SECOND_MS, 0), &[])
        .expect("prices")
        .expect("runtime logs a line");
    let Attribution::Fleet { fleet_name, .. } = &settlement.attribution else {
        unreachable!("a lease's line is attributed to its fleet");
    };
    assert_eq!(fleet_name.as_str(), test_dead::id(1).as_str());
}

/// A pass with no requests and no run time logs nothing.
#[tokio::test]
async fn an_empty_pass_logs_no_line() {
    let pricing = pricing(Posture::SelfManaged).await;
    assert!(
        pricing
            .settlement(&test_dead::id(4), slice(0, 0), &[])
            .expect("prices")
            .is_none()
    );
}
