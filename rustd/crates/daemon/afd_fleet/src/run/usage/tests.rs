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
use crate::run::test_dead;

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
        turn_id: test_dead::id(5),
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
        "run-fixture",
    )
    .await
}

/// A catalogue that cannot be read still logs every request: its tokens are
/// counted unpriced on the line, and the warning names the run.
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
    assert_eq!(event.field("run_id"), Some("run-fixture"));
}

/// A slice longer than the longest runtime one line may claim is cut there, as
/// the renewal and report statements cut theirs, so the three charge alike.
#[tokio::test]
async fn a_slice_past_the_longest_runtime_line_is_cut_there() {
    let pricing = pricing(Posture::SelfManaged).await;
    let past = i64::from(u32::MAX) + ONE_SECOND_MS;
    let settlement = pricing
        .settlement(&test_dead::id(4), slice(past, 0), &[])
        .expect("prices")
        .expect("a runtime line");
    let line = settlement.lines.first().expect("one line");
    assert_eq!(line.usage.runtime_ms, u32::MAX);
}

/// A stored posture this daemon writes reads back; one it does not is refused,
/// never priced as the platform's.
///
/// The refused spellings are the near-misses an out-of-band edit actually
/// produces: a case change, a hyphen for an underscore, a leading space, the
/// empty string, and a spelling borrowed from another column. A parser that
/// trimmed or lowercased would accept them, and a run would then be priced
/// at the wrong party's rates.
#[test]
fn a_stored_posture_that_is_none_this_daemon_writes_is_refused() {
    for posture in [Posture::Platform, Posture::SelfManaged] {
        assert_eq!(
            Pricing::posture_of(posture.as_str()).expect("a written posture reads back"),
            posture
        );
    }
    for edited in [
        "",
        "Platform",
        "self-managed",
        "sandboxed",
        " platform",
        "hand-edited",
    ] {
        let fault = Pricing::posture_of(edited).expect_err("an unreadable posture");
        assert_eq!(
            fault.code(),
            af_core::error_code::INTERNAL_DB_QUERY,
            "{edited:?} is refused, not priced"
        );
    }
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
        work: WorkKey::FleetRun(test_dead::id(4)),
        payer: Account {
            tenant: test_dead::id(2),
            workspace: test_dead::id(3),
        },
        attribution: Attribution::Fleet {
            fleet: test_dead::id(1),
            fleet_name: test_dead::id(1).as_str().to_owned(),
            turn_id: test_dead::id(5),
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
        unreachable!("a run's line is attributed to its fleet");
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

/// A book whose one tier charges the most a signed 64-bit rate allows, so a
/// full-width request cannot be priced in a signed 64-bit nano count.
fn extreme() -> Pricing {
    let tier = afd_billing::price::Tier {
        id: test_dead::id(8),
        price: af_core::catalogue::Price {
            effective_from: jiff::Timestamp::UNIX_EPOCH,
            effective_until: None,
            prompt_tokens_above: 0,
            input_nanos_per_mtok: i64::MAX,
            cached_input_nanos_per_mtok: i64::MAX,
            cache_write_nanos_per_mtok: None,
            cache_write_5m_nanos_per_mtok: None,
            cache_write_1h_nanos_per_mtok: None,
            output_nanos_per_mtok: i64::MAX,
            source_url: "https://example.invalid/pricing".to_owned(),
            verified_at: jiff::Timestamp::UNIX_EPOCH,
            conditions: String::new(),
        },
    };
    Pricing::of(
        afd_billing::price::PriceBook::from_tiers(vec![tier]),
        Posture::Platform,
        "anthropic",
        "claude-opus-5-5",
    )
}

/// A request at the wire's widest token counts.
fn widest() -> RequestUsage {
    RequestUsage {
        input_tokens: u32::MAX,
        output_tokens: u32::MAX,
        ..request(0)
    }
}

// Value: protects=a charge that cannot fit a signed 64-bit nano count is refused
// on every pricing path, never wrapped or logged as free; fails_when=either path
// swallows the rater's refusal; why_new=no fleet pricing path met an overflowing
// tier; seam=`Pricing::request_lines` and `Pricing::settlement`.
#[test]
fn a_charge_past_a_signed_64_bit_count_is_refused_on_every_path() {
    let pricing = extreme();

    let lines = pricing
        .request_lines(test_dead::AT, test_dead::AT, &[widest()])
        .expect_err("the report's request lines refuse an overflow");
    let settled = pricing
        .settlement(&test_dead::id(4), slice(0, 0), &[widest()])
        .expect_err("a renewal's settlement refuses an overflow");

    for fault in [lines, settled] {
        assert_eq!(fault.code(), af_core::error_code::INTERNAL_OPERATION_FAILED);
        assert!(!fault.is_datastore_unavailable());
    }
    assert_eq!(
        pricing
            .request_lines(test_dead::AT, test_dead::AT, &[])
            .expect("no request prices to no line")
            .len(),
        0
    );
}

// Value: protects=a setup run's pass with neither requests nor run time logs no
// line and spends no line number; fails_when=the draft settlement answers an
// empty settlement; why_new=only the fleet settlement's empty pass was tested;
// seam=`Pricing::draft_settlement`.
#[tokio::test]
async fn an_empty_setup_pass_logs_no_line() {
    let pricing = pricing(Posture::SelfManaged).await;
    let slice = super::DraftSlice {
        runtime_seq: 0,
        checked_at: test_dead::AT,
        started_at: test_dead::AT,
        runtime_ms: 0,
        tenant_id: test_dead::id(2),
        workspace_id: test_dead::id(3),
        message_id: test_dead::id(5),
    };

    assert!(
        pricing
            .draft_settlement(&test_dead::id(4), slice, &[])
            .expect("prices")
            .is_none()
    );
}
