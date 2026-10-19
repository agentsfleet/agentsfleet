#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::catalogue::Price;
use afd_core::id::Uuid7;
use afd_wire::usage::UsageLine;
use jiff::Timestamp;

use super::{PriceBook, Rater, Tier, Window};
use crate::{NANOS_PER_USD, Posture};

const BASE_ID: &str = "01990000-0000-7000-8000-000000000001";
const UPPER_ID: &str = "01990000-0000-7000-8000-000000000002";
const LATER_ID: &str = "01990000-0000-7000-8000-000000000003";
/// When the fixture tiers are first published.
const YEAR_START: &str = "2026-01-01T00:00:00Z";
/// A request time inside every open fixture range.
const MIDYEAR: &str = "2026-06-01T00:00:00Z";
/// The UTC midnight a fixture price changes at.
const DECEMBER: &str = "2026-12-01T00:00:00Z";
/// The last millisecond before [`DECEMBER`].
const LAST_NOVEMBER_INSTANT: &str = "2026-11-30T23:59:59.999Z";
/// One million tokens: the unit every rate is published per.
const MILLION: u32 = 1_000_000;
/// A tenth of a million tokens, under the long-context threshold.
const TENTH: u32 = 100_000;
/// A base input price of 0.10 USD per million, in hundredths of a cent.
const DIME: i64 = 1_000;
/// A base output price of 10.00 USD per million, in hundredths of a cent.
const TEN_DOLLARS: i64 = 100_000;
/// Runtime a self-managed line reports.
const RUNTIME_MS: u32 = 2_500;
/// Sub-cent rates (0.006, 0.015, 0.0375, 0.0625, 0.075, 0.125 and 0.1875 USD
/// per million) in hundredths of a cent, and the exact nanos per million each
/// must store and bill.
const SUB_CENT_RATES: [(i64, i64); 7] = [
    (60, 6_000_000),
    (150, 15_000_000),
    (375, 37_500_000),
    (625, 62_500_000),
    (750, 75_000_000),
    (1_250, 125_000_000),
    (1_875, 187_500_000),
];
/// A long-context threshold: 272,000 prompt tokens.
const THRESHOLD: i32 = 272_000;
/// Haiku 5.5's long-context threshold: 100,000 prompt tokens, cache included.
const HAIKU_THRESHOLD: i32 = 100_000;
/// Hundredths of a cent per million tokens, as nanos per million tokens.
const fn usd_per_mtok(cents_hundredths: i64) -> i64 {
    cents_hundredths * NANOS_PER_USD / 10_000
}

fn at(text: &str) -> Timestamp {
    text.parse().expect("the fixture instant parses")
}

fn price(
    from: &str,
    until: Option<&str>,
    above: i32,
    input: i64,
    cached: i64,
    output: i64,
) -> Price {
    Price {
        effective_from: at(from),
        effective_until: until.map(at),
        prompt_tokens_above: above,
        input_nanos_per_mtok: input,
        cached_input_nanos_per_mtok: cached,
        cache_write_nanos_per_mtok: None,
        cache_write_5m_nanos_per_mtok: Some(input * 5 / 4),
        cache_write_1h_nanos_per_mtok: Some(input * 2),
        output_nanos_per_mtok: output,
        source_url: "https://example.invalid/pricing".to_owned(),
        verified_at: at("2026-10-18T12:33:00Z"),
        conditions: String::new(),
    }
}

fn tier(id: &str, price: Price) -> Tier {
    Tier {
        id: Uuid7::parse(id).expect("the fixture id is a UUIDv7"),
        price,
    }
}

fn line(at_text: &str, input: u32, output: u32) -> UsageLine {
    UsageLine {
        seq: 0,
        at: at(at_text),
        input_tokens: input,
        cached_input_tokens: 0,
        cache_write_tokens: 0,
        cache_write_5m_tokens: 0,
        cache_write_1h_tokens: 0,
        output_tokens: output,
        runtime_ms: 0,
    }
}

const fn wide() -> Window {
    Window {
        from: Timestamp::constant(0, 0),
        until: Timestamp::constant(4_102_444_800, 0),
    }
}

fn tiered() -> PriceBook {
    PriceBook::from_tiers(vec![
        tier(
            BASE_ID,
            price(
                YEAR_START,
                None,
                0,
                usd_per_mtok(25_000),
                usd_per_mtok(2_500),
                usd_per_mtok(150_000),
            ),
        ),
        tier(
            UPPER_ID,
            price(
                YEAR_START,
                None,
                THRESHOLD,
                usd_per_mtok(50_000),
                usd_per_mtok(5_000),
                usd_per_mtok(225_000),
            ),
        ),
    ])
}

#[test]
fn test_context_tier_prices_the_whole_request() {
    let book = tiered();
    let rater = Rater::new(&book, Posture::Platform, wide());
    let at_edge = rater.rate(&line(MIDYEAR, 272_000, 0)).expect("prices");
    let past = rater.rate(&line(MIDYEAR, 272_001, 0)).expect("prices");
    assert_eq!(at_edge.price, Some(Uuid7::parse(BASE_ID).expect("id")));
    assert_eq!(past.price, Some(Uuid7::parse(UPPER_ID).expect("id")));
    // The upper rate applies to EVERY token of the request, not only those past
    // the threshold: 272,001 × 5.00 USD/Mtok, rounded up.
    assert_eq!(past.charged.as_i64(), 1_360_005_000);
}

#[test]
fn test_context_tier_counts_cache_reads_toward_the_haiku_threshold() {
    // Haiku 5.5: 0.10 / 0.01 / 0.50 at base, 0.50 / 0.05 / 2.50 above 100,000.
    let book = PriceBook::from_tiers(vec![
        tier(
            BASE_ID,
            price(
                YEAR_START,
                None,
                0,
                usd_per_mtok(DIME),
                usd_per_mtok(100),
                usd_per_mtok(5_000),
            ),
        ),
        tier(
            UPPER_ID,
            price(
                YEAR_START,
                None,
                HAIKU_THRESHOLD,
                usd_per_mtok(5_000),
                usd_per_mtok(500),
                usd_per_mtok(25_000),
            ),
        ),
    ]);
    let rater = Rater::new(&book, Posture::Platform, wide());
    let mut at_edge = line(MIDYEAR, 99_000, 0);
    at_edge.cached_input_tokens = 1_000;
    let mut past = at_edge;
    past.cached_input_tokens = 1_001;

    let at_edge = rater.rate(&at_edge).expect("prices");
    let past = rater.rate(&past).expect("prices");
    // 100,000 prompt tokens is not ABOVE the threshold: base on every token.
    // 99,000 × 0.10 + 1,000 × 0.01 USD/Mtok.
    assert_eq!(at_edge.price, Some(Uuid7::parse(BASE_ID).expect("id")));
    assert_eq!(at_edge.charged.as_i64(), 9_910_000);
    // 100,001 is: the upper rate prices the whole request, cache read included.
    // 99,000 × 0.50 + 1,001 × 0.05 USD/Mtok.
    assert_eq!(past.price, Some(Uuid7::parse(UPPER_ID).expect("id")));
    assert_eq!(past.charged.as_i64(), 49_550_050);
}

#[test]
fn test_rater_prices_each_line_by_tier_and_date() {
    let book = tiered();
    let rater = Rater::new(&book, Posture::Platform, wide());
    let mut cached = line(MIDYEAR, 271_000, 0);
    cached.cached_input_tokens = 500;
    cached.cache_write_5m_tokens = 501;
    // 271,000 + 500 + 501 crosses the threshold: cache reads and writes count.
    let priced = rater.rate(&cached).expect("prices");
    assert_eq!(priced.price, Some(Uuid7::parse(UPPER_ID).expect("id")));

    // One token at 0.006 USD per million is 6 nanos exactly.
    let sub_cent = PriceBook::from_tiers(vec![tier(
        BASE_ID,
        price(YEAR_START, None, 0, usd_per_mtok(60), 0, 0),
    )]);
    let rater = Rater::new(&sub_cent, Posture::Platform, wide());
    let one = rater.rate(&line(MIDYEAR, 1, 0)).expect("prices");
    assert_eq!(one.charged.as_i64(), 6);
}

#[test]
fn test_effective_dates_switch_at_utc_midnight() {
    let book = PriceBook::from_tiers(vec![
        tier(
            BASE_ID,
            price(
                YEAR_START,
                Some(DECEMBER),
                0,
                usd_per_mtok(20_000),
                usd_per_mtok(DIME),
                usd_per_mtok(TEN_DOLLARS),
            ),
        ),
        tier(
            LATER_ID,
            price(
                DECEMBER,
                None,
                0,
                usd_per_mtok(40_000),
                usd_per_mtok(2_000),
                usd_per_mtok(200_000),
            ),
        ),
    ]);
    let rater = Rater::new(&book, Posture::Platform, wide());
    let before = rater
        .rate(&line(LAST_NOVEMBER_INSTANT, MILLION, 0))
        .expect("prices");
    let after = rater.rate(&line(DECEMBER, MILLION, 0)).expect("prices");
    assert_eq!(before.charged.as_i64(), 2_000_000_000);
    assert_eq!(after.charged.as_i64(), 4_000_000_000);

    // A million cached and a million output tokens, each side of midnight:
    // 0.10 → 0.20 and 10.00 → 20.00 USD per million.
    let cached = |at_text: &str| {
        let mut cached = line(at_text, 0, 0);
        cached.cached_input_tokens = MILLION;
        rater.rate(&cached).expect("prices").charged.as_i64()
    };
    let output = |at_text: &str| {
        rater
            .rate(&line(at_text, 0, MILLION))
            .expect("prices")
            .charged
            .as_i64()
    };
    assert_eq!(cached(LAST_NOVEMBER_INSTANT), 100_000_000);
    assert_eq!(cached(DECEMBER), 200_000_000);
    assert_eq!(output(LAST_NOVEMBER_INSTANT), 10_000_000_000);
    assert_eq!(output(DECEMBER), 20_000_000_000);
}

#[test]
fn test_cache_writes_bill_at_their_rate() {
    let book = tiered();
    let rater = Rater::new(&book, Posture::Platform, wide());
    let mut writes = line(MIDYEAR, 0, 0);
    // 200,000 write tokens stay under the threshold, so the base tier applies.
    writes.cache_write_5m_tokens = TENTH;
    writes.cache_write_1h_tokens = TENTH;
    let priced = rater.rate(&writes).expect("prices");
    // A tenth of a million each, at 2.50 × 1.25 and at 2.50 × 2, never at the
    // uncached input rate.
    assert_eq!(
        priced.charged.as_i64(),
        (usd_per_mtok(31_250) + usd_per_mtok(50_000)) / 10
    );
    assert_eq!(priced.unpriced_tokens, 0);

    // OpenAI names no lifetime and publishes one write rate per prompt tier,
    // 1.25 × that tier's input: 2.50 USD under 272K, 5.00 USD over it.
    let lifetimeless = |above: i32, input: i64| {
        let mut price = price(YEAR_START, None, above, input, input / 10, input * 6);
        price.cache_write_nanos_per_mtok = Some(input * 5 / 4);
        price.cache_write_5m_nanos_per_mtok = None;
        price.cache_write_1h_nanos_per_mtok = None;
        price
    };
    let openai = PriceBook::from_tiers(vec![
        tier(BASE_ID, lifetimeless(0, usd_per_mtok(20_000))),
        tier(UPPER_ID, lifetimeless(THRESHOLD, usd_per_mtok(40_000))),
    ]);
    let rater = Rater::new(&openai, Posture::Platform, wide());
    let mut under = line(MIDYEAR, 0, 0);
    under.cache_write_tokens = MILLION / 10;
    let mut over = line(MIDYEAR, 0, 0);
    over.cache_write_tokens = 3 * MILLION / 10;
    let under = rater.rate(&under).expect("prices");
    let over = rater.rate(&over).expect("prices");
    assert_eq!(under.charged.as_i64(), usd_per_mtok(25_000) / 10);
    assert_eq!(over.charged.as_i64(), 3 * usd_per_mtok(50_000) / 10);
    assert_eq!(over.price, Some(Uuid7::parse(UPPER_ID).expect("id")));
    assert_eq!(over.unpriced_tokens, 0, "never as uncached input");
}

#[test]
fn a_cache_write_with_no_published_rate_is_unpriced_not_free() {
    let book = tiered();
    let rater = Rater::new(&book, Posture::Platform, wide());
    let mut writes = line(MIDYEAR, 0, 0);
    writes.cache_write_tokens = 1_234;
    let priced = rater.rate(&writes).expect("prices");
    assert_eq!(priced.unpriced_tokens, 1_234);
    assert_eq!(priced.charged.as_i64(), 0);
}

#[test]
fn test_sub_cent_rates_bill_exactly() {
    for (hundredths_of_a_cent, expected_nanos) in SUB_CENT_RATES {
        let per_mtok = usd_per_mtok(hundredths_of_a_cent);
        assert_eq!(per_mtok, expected_nanos);
        let book = PriceBook::from_tiers(vec![tier(
            BASE_ID,
            price(YEAR_START, None, 0, per_mtok, 0, 0),
        )]);
        let rater = Rater::new(&book, Posture::Platform, wide());
        let million = rater.rate(&line(MIDYEAR, MILLION, 0)).expect("prices");
        assert_eq!(million.charged.as_i64(), expected_nanos);
    }
}

#[test]
fn every_category_rounds_up_on_its_own() {
    // 37.5 nanos per token: one token of input and one of output each round up.
    let per_mtok = usd_per_mtok(375);
    let book = PriceBook::from_tiers(vec![tier(
        BASE_ID,
        price(YEAR_START, None, 0, per_mtok, 0, per_mtok),
    )]);
    let rater = Rater::new(&book, Posture::Platform, wide());
    let priced = rater.rate(&line(MIDYEAR, 1, 1)).expect("prices");
    assert_eq!(priced.charged.as_i64(), 76);
}

#[test]
fn test_request_time_is_clamped_to_the_work() {
    let book = PriceBook::from_tiers(vec![
        tier(
            BASE_ID,
            price(YEAR_START, Some(DECEMBER), 0, usd_per_mtok(10_000), 0, 0),
        ),
        tier(
            LATER_ID,
            price(DECEMBER, None, 0, usd_per_mtok(90_000), 0, 0),
        ),
    ]);
    let window = Window {
        from: at("2026-12-01T10:00:00Z"),
        until: at("2026-12-01T10:05:00Z"),
    };
    let rater = Rater::new(&book, Posture::Platform, window);
    let backdated = rater
        .rate(&line("2026-11-01T00:00:00Z", MILLION, 0))
        .expect("prices");
    assert_eq!(backdated.usage.at, window.from);
    assert_eq!(backdated.price, Some(Uuid7::parse(LATER_ID).expect("id")));
    let late = rater
        .rate(&line("2027-01-01T00:00:00Z", 1, 0))
        .expect("prices");
    assert_eq!(late.usage.at, window.until);
}

#[test]
fn self_managed_work_charges_runtime_only() {
    let book = tiered();
    let rater = Rater::new(&book, Posture::SelfManaged, wide());
    let mut ran = line(MIDYEAR, 900_000, 900_000);
    ran.runtime_ms = RUNTIME_MS;
    let priced = rater.rate(&ran).expect("prices");
    assert_eq!(priced.price, None);
    assert_eq!(priced.unpriced_tokens, 0);
    assert_eq!(
        priced.charged.as_i64(),
        i64::from(RUNTIME_MS) * crate::RUN_NANOS_PER_SEC / crate::nanos::MS_PER_SEC
    );
}

#[test]
fn a_request_no_published_range_covers_is_entirely_unpriced() {
    let book = PriceBook::default();
    let rater = Rater::new(&book, Posture::Platform, wide());
    let priced = rater.rate(&line(MIDYEAR, 10, 3)).expect("prices");
    assert_eq!(priced.price, None);
    assert_eq!(priced.unpriced_tokens, 13);
    assert_eq!(priced.charged.as_i64(), 0);
}

#[test]
fn a_charge_past_a_signed_64_bit_count_is_refused() {
    let mut extreme = price(YEAR_START, None, 0, 0, 0, 0);
    extreme.input_nanos_per_mtok = i64::MAX;
    extreme.output_nanos_per_mtok = i64::MAX;
    let book = PriceBook::from_tiers(vec![tier(BASE_ID, extreme)]);
    let rater = Rater::new(&book, Posture::Platform, wide());
    rater
        .rate(&line(MIDYEAR, u32::MAX, u32::MAX))
        .expect_err("an overflowing charge is refused");
}
