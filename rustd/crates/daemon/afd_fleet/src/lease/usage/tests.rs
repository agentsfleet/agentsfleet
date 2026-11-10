#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use af_core::test_util::trace::Capture;
use af_wire::usage::{LineKind, RequestUsage, UsageLine};
use afd_billing::rates::Posture;
use afd_billing::{
    Nanos,
    biller::{Account, Attribution, Settlement, WorkKey},
    price::RatedLine,
};
use tracing::Level;

use super::{EVENT_TOKENS_UNPRICED, Pricing, Slice, charged};
use crate::lease::test_dead;

/// One second of runtime.
const ONE_SECOND_MS: i64 = 1_000;

fn slice(runtime_ms: i64, runtime_seq: i64) -> Slice {
    Slice {
        runtime_seq,
        checked_at: test_dead::AT,
        started_at: test_dead::AT,
        runtime_ms,
        tenant_id: test_dead::id(2),
        workspace_id: test_dead::id(3),
        fleet_id: test_dead::id(1),
        fleet_name: None,
        event_id: test_dead::id(5),
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

/// Each request is a model line under the runner's number, and the measured
/// time is one runtime line numbered by its slice's start; a self-managed run
/// pays the run fee only.
#[tokio::test]
async fn requests_keep_their_numbers_and_runtime_is_numbered_by_its_slice() {
    let pricing = pricing(Posture::SelfManaged).await;
    let settlement = pricing
        .settlement(
            &test_dead::id(4),
            slice(ONE_SECOND_MS, 2_000),
            &[request(0), request(1)],
        )
        .expect("prices")
        .expect("lines");
    let numbers: Vec<(LineKind, u32)> = settlement
        .lines
        .iter()
        .map(|line| (line.usage.kind, line.usage.seq))
        .collect();
    assert_eq!(
        numbers,
        [
            (LineKind::Model, 0),
            (LineKind::Model, 1),
            (LineKind::Runtime, 2_000)
        ]
    );
    assert_eq!(
        charged(Some(&settlement)).as_i64(),
        afd_billing::RUN_NANOS_PER_SEC,
        "one second of runtime is the whole self-managed charge"
    );
}

/// A model line and a runtime line may carry the same number: their kinds keep
/// them apart, at both ends of the `u32`. A slice start that does not fit one
/// is a corrupt row, never a wrapped number.
#[tokio::test]
async fn model_and_runtime_number_independently_and_refuse_an_unfit_slice() {
    let pricing = pricing(Posture::SelfManaged).await;
    for runtime_seq in [0, i64::from(u32::MAX)] {
        let expected_seq = u32::try_from(runtime_seq).expect("the case fits");
        let actual = pricing
            .settlement(
                &test_dead::id(4),
                slice(ONE_SECOND_MS, runtime_seq),
                &[request(expected_seq)],
            )
            .expect("model and runtime number independently")
            .expect("both writers produce a line");
        assert_eq!(actual, expected_boundary(expected_seq));
    }
    for runtime_seq in [-1, i64::from(u32::MAX) + 1, i64::MAX] {
        let failure = pricing
            .settlement(
                &test_dead::id(4),
                slice(ONE_SECOND_MS, runtime_seq),
                &[request(0)],
            )
            .expect_err("a slice start the wire cannot carry is refused");
        assert_eq!(failure.code(), af_core::error_code::INTERNAL_DB_QUERY);
        assert!(!failure.is_config_permanent());
        assert_eq!(
            pricing
                .settlement(&test_dead::id(4), slice(0, runtime_seq), &[])
                .expect("an empty pass allocates no sequence"),
            None
        );
    }
}

fn expected_boundary(seq: u32) -> Settlement {
    let uncharged = |usage| RatedLine {
        usage,
        price: None,
        charged: Nanos::ZERO,
        unpriced_tokens: 0,
    };
    let coding = uncharged(UsageLine {
        kind: LineKind::Model,
        seq,
        at: test_dead::AT,
        input_tokens: 25_000,
        cached_input_tokens: 2_500,
        cache_write_tokens: 0,
        cache_write_5m_tokens: 0,
        cache_write_1h_tokens: 0,
        output_tokens: 4_000,
        runtime_ms: 0,
    });
    let mut runtime = uncharged(UsageLine {
        kind: LineKind::Runtime,
        seq,
        at: test_dead::AT,
        input_tokens: 0,
        cached_input_tokens: 0,
        cache_write_tokens: 0,
        cache_write_5m_tokens: 0,
        cache_write_1h_tokens: 0,
        output_tokens: 0,
        runtime_ms: 1_000,
    });
    runtime.charged = Nanos::from_i64(afd_billing::RUN_NANOS_PER_SEC);
    Settlement {
        work: WorkKey::FleetLease(test_dead::id(4)),
        payer: Account {
            tenant: test_dead::id(2),
            workspace: test_dead::id(3),
        },
        attribution: Attribution::Fleet {
            fleet: test_dead::id(1),
            fleet_name: test_dead::id(1).as_str().to_owned(),
            event_id: test_dead::id(5),
        },
        provider: "anthropic".to_owned(),
        model: "claude-opus-5-5".to_owned(),
        posture: Posture::SelfManaged,
        lines: vec![coding, runtime],
    }
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
