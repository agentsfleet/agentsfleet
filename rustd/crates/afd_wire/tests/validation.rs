//! What the declared bounds actually refuse, and what a malformed body does.
//!
//! # Two properties, two techniques, on purpose
//!
//! **The bounds are enumerable, so they are tested exactly.** Every
//! `garde(length)` and `garde(range)` in this crate names a constant. A value
//! at the limit must be accepted and a value one past it refused, and those two
//! points are the entire question — a generator drawing random strings would
//! have to be astronomically lucky to land on either, so random search is the
//! weaker tool here, not the stronger one. Each row below is that pair.
//!
//! **The parser's input is not enumerable, so it is generated.** Nobody can
//! list the malformed documents a peer might send. What matters there is a
//! single property that must hold for all of them: a bad body produces an
//! `Err`, never a panic, never an abort, never a half-built value. The second
//! half of this file throws several thousand mutations at the parser and
//! asserts exactly that.
//!
//! # Why the generator is seeded rather than random
//!
//! A test that draws fresh randomness each run fails on one machine, passes on
//! the next, and gives whoever reads the failure nothing to reproduce it with.
//! The generator here is a fixed-seed xorshift: the same several thousand
//! inputs every run, on every machine, in Continuous Integration (CI) and
//! locally. Widening coverage means raising [`MUTATION_ROUNDS`] or adding a
//! seed, both of which are a diff someone reviews, rather than a surprise.
//!
//! It is not a substitute for coverage-guided fuzzing. libFuzzer explores the
//! parser's branches; this asserts a property over a fixed corpus of mutations.
//! If a panic ever surfaces here it is real, and if one never does that is not
//! proof the parser is total.
//!
//! # What this does NOT cover
//!
//! `garde` runs at the service boundary, not in this crate — `afd_wire` is the
//! definition and validation belongs where a request arrives. These rows prove
//! the DECLARATION rejects what it says it rejects. They do not prove any
//! handler calls `validate()` on its way in; that is the call site's own test.
//! For the runner path that call site is `afd_runner::bounds::accept`, reached
//! from `heartbeat.rs`.
//!
//! Nor is this coverage-guided fuzzing. libFuzzer explores a parser's branches
//! and would find inputs a fixed corpus never reaches. Adding it means a
//! nightly toolchain and an out-of-band runner, so it cannot ride
//! `make test-unit-all`; that remains open.

#![expect(
    clippy::unwrap_used,
    reason = "test target: a value this file built wrong is an unmet \
              precondition, and failing loudly on it is the correct outcome"
)]

use std::borrow::Cow;

use afd_wire::event::{OPERATION_ID_MAX_BYTES, STEER_MESSAGE_MAX_BYTES, SteerRequest};
use afd_wire::runner::{
    CHECK_DETAIL_MAX_BYTES, CHECK_NAME_MAX_BYTES, SELFTEST_CHECKS_MAX, SELFTEST_POLICY_MAX_BYTES,
    SelftestCheck, SelftestReport,
};
use garde::Validate as _;

/// How many mutations the parser survives per seed corpus entry.
///
/// Sized to run in well under a second while still exercising every mutation
/// arm many times over. Raising it is a review-able diff, which is the point.
const MUTATION_ROUNDS: usize = 4_000;

/// A string of exactly `bytes` ASCII characters.
///
/// ASCII deliberately: `garde(length(bytes, ...))` counts BYTES, so building
/// the probe from multi-byte characters would make the boundary arithmetic a
/// second thing to get right and a failure ambiguous between the two.
fn of_len(bytes: usize) -> String {
    "a".repeat(bytes)
}

fn check_named(name: String) -> SelftestCheck<'static> {
    SelftestCheck {
        name: Cow::Owned(name),
        ok: true,
        detail: Cow::Borrowed("ok"),
    }
}

fn report_with(checks: Vec<SelftestCheck<'static>>) -> SelftestReport<'static> {
    SelftestReport {
        checks,
        all_ok: true,
        sandbox_tier: Cow::Borrowed("landlock_full"),
        network_policy: Cow::Borrowed("allow_list_egress"),
    }
}

/// The bounds themselves, pinned to their values.
///
/// Every other row in this file builds its probe from the same constant it
/// asserts against, so the pair moves together: widen `CHECK_NAME_MAX_BYTES` to
/// a megabyte and those rows stay green while the cap they were written to
/// defend is gone. Proven, not assumed — loosening that constant to 100000 left
/// the whole file passing before this row existed.
///
/// So the numbers are written out once, here. A bound may absolutely be moved;
/// this makes moving it a line in a diff someone reviews rather than a silent
/// widening.
#[test]
fn the_declared_bounds_are_the_numbers_this_wire_was_designed_around() {
    assert_eq!(CHECK_NAME_MAX_BYTES, 128, "CHECK_NAME_MAX_BYTES");
    assert_eq!(CHECK_DETAIL_MAX_BYTES, 256, "CHECK_DETAIL_MAX_BYTES");
    assert_eq!(SELFTEST_CHECKS_MAX, 32, "SELFTEST_CHECKS_MAX");
    assert_eq!(SELFTEST_POLICY_MAX_BYTES, 64, "SELFTEST_POLICY_MAX_BYTES");
    assert_eq!(STEER_MESSAGE_MAX_BYTES, 8192, "STEER_MESSAGE_MAX_BYTES");
    assert_eq!(OPERATION_ID_MAX_BYTES, 200, "OPERATION_ID_MAX_BYTES");
}

/// A check whose every field sits exactly at its limit is accepted, and one
/// byte past any of them is refused.
///
/// `SelftestCheck` is the type a compromised or buggy host has the most direct
/// reach into: the name and detail are prose it chooses, and they land in an
/// operator's log. The bound is what stops a host writing a megabyte there.
#[test]
fn a_selftest_check_accepts_its_limits_and_refuses_one_byte_past_them() {
    let at_limit = SelftestCheck {
        name: Cow::Owned(of_len(CHECK_NAME_MAX_BYTES)),
        ok: true,
        detail: Cow::Owned(of_len(CHECK_DETAIL_MAX_BYTES)),
    };
    assert!(at_limit.validate().is_ok(), "a check at its limits");

    let long_name = SelftestCheck {
        name: Cow::Owned(of_len(CHECK_NAME_MAX_BYTES + 1)),
        ..at_limit.clone()
    };
    assert!(long_name.validate().is_err(), "name one byte past its cap");

    let long_detail = SelftestCheck {
        detail: Cow::Owned(of_len(CHECK_DETAIL_MAX_BYTES + 1)),
        ..at_limit.clone()
    };
    assert!(
        long_detail.validate().is_err(),
        "detail one byte past its cap"
    );
}

/// The `min = 1` half, which is a different failure from the cap.
///
/// An empty name is not a short name, it is a check that names nothing — the
/// operator reading the log learns which check failed from this field and
/// nowhere else.
#[test]
fn a_selftest_check_refuses_an_empty_name_or_detail() {
    let empty_name = check_named(String::new());
    assert!(empty_name.validate().is_err(), "an empty name");

    let empty_detail = SelftestCheck {
        name: Cow::Borrowed("landlock"),
        ok: true,
        detail: Cow::Borrowed(""),
    };
    assert!(empty_detail.validate().is_err(), "an empty detail");
}

/// A report carrying its maximum number of checks is accepted; one more is not.
#[test]
fn a_selftest_report_accepts_its_full_roster_and_refuses_one_more() {
    let full = report_with(
        (0..SELFTEST_CHECKS_MAX)
            .map(|index| check_named(format!("check_{index}")))
            .collect(),
    );
    assert!(full.validate().is_ok(), "a full roster");

    let over = report_with(
        (0..=SELFTEST_CHECKS_MAX)
            .map(|index| check_named(format!("check_{index}")))
            .collect(),
    );
    assert!(over.validate().is_err(), "one check past the roster cap");
}

/// `dive` reaches inside the roster, so a bad check fails the whole report.
///
/// This is the row that proves `dive` is doing something. Without it a report
/// could carry a check with a megabyte name and pass, because the outer type
/// only counts the roster.
#[test]
fn one_oversized_check_inside_a_legal_roster_still_fails_the_report() {
    let smuggled = report_with(vec![
        check_named("fine".to_owned()),
        check_named(of_len(CHECK_NAME_MAX_BYTES + 1)),
    ]);
    assert!(
        smuggled.validate().is_err(),
        "an oversized check inside a roster within its count"
    );
}

/// The report's own policy strings carry the same at-limit / one-past pair.
#[test]
fn a_selftest_report_bounds_the_policy_strings_it_echoes() {
    let at_limit = SelftestReport {
        checks: vec![check_named("landlock".to_owned())],
        all_ok: true,
        sandbox_tier: Cow::Owned(of_len(SELFTEST_POLICY_MAX_BYTES)),
        network_policy: Cow::Owned(of_len(SELFTEST_POLICY_MAX_BYTES)),
    };
    assert!(at_limit.validate().is_ok(), "policy strings at their limit");

    let over = SelftestReport {
        sandbox_tier: Cow::Owned(of_len(SELFTEST_POLICY_MAX_BYTES + 1)),
        ..at_limit.clone()
    };
    assert!(over.validate().is_err(), "a policy string past its cap");

    let empty = SelftestReport {
        network_policy: Cow::Borrowed(""),
        ..at_limit
    };
    assert!(empty.validate().is_err(), "an empty policy string");
}

/// A steer message is bounded, and its optional operation id is bounded only
/// when present.
///
/// `garde(inner(...))` is the arm that makes `None` legal while a present value
/// is still held to its bounds — the distinction a hand-written check gets
/// wrong by rejecting absence.
#[test]
fn a_steer_request_bounds_its_message_and_its_optional_operation_id() {
    let at_limit = SteerRequest {
        message: Cow::Owned(of_len(STEER_MESSAGE_MAX_BYTES)),
        operation_id: Some(Cow::Owned(of_len(OPERATION_ID_MAX_BYTES))),
    };
    assert!(at_limit.validate().is_ok(), "a steer at its limits");

    let absent_id = SteerRequest {
        message: Cow::Borrowed("restart the run"),
        operation_id: None,
    };
    assert!(absent_id.validate().is_ok(), "an absent operation id");

    let long_message = SteerRequest {
        message: Cow::Owned(of_len(STEER_MESSAGE_MAX_BYTES + 1)),
        operation_id: None,
    };
    assert!(long_message.validate().is_err(), "a message past its cap");

    let empty_message = SteerRequest {
        message: Cow::Borrowed(""),
        operation_id: None,
    };
    assert!(empty_message.validate().is_err(), "an empty message");

    let long_id = SteerRequest {
        message: Cow::Borrowed("restart the run"),
        operation_id: Some(Cow::Owned(of_len(OPERATION_ID_MAX_BYTES + 1))),
    };
    assert!(long_id.validate().is_err(), "an operation id past its cap");

    let empty_id = SteerRequest {
        message: Cow::Borrowed("restart the run"),
        operation_id: Some(Cow::Borrowed("")),
    };
    assert!(empty_id.validate().is_err(), "a present but empty id");
}

/// A fixed-seed xorshift64*, so every run sees the same corpus.
///
/// Hand-rolled rather than pulled in: the generator needs to be reproducible
/// and nothing more, and a dependency whose whole job is three lines of shift
/// arithmetic is a dependency to keep current for no gain.
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, ceiling: usize) -> usize {
        usize::try_from(self.next() % (ceiling as u64)).unwrap()
    }
}

/// Valid documents the mutator starts from, one per shape the wire carries.
///
/// Starting from valid input matters: random bytes are rejected by the first
/// token and never reach the field-walking code, while a document that is
/// nearly right reaches deep and is where a parser actually breaks.
const SEED_CORPUS: [&str; 4] = [
    r#"{"message":"restart","operation_id":"op_1"}"#,
    r#"{"name":"landlock","ok":true,"detail":"applied"}"#,
    r#"{"checks":[{"name":"a","ok":true,"detail":"d"}],"all_ok":true,"sandbox_tier":"t","network_policy":"n"}"#,
    r#"{"input_tokens":1,"cached_input_tokens":0,"output_tokens":0}"#,
];

/// One mutation of `source`, chosen by `rng`.
fn mutate(rng: &mut Seeded, source: &str) -> Vec<u8> {
    let mut bytes = source.as_bytes().to_vec();
    if bytes.is_empty() {
        return bytes;
    }
    match rng.below(6) {
        // Truncate: the shape a dropped connection leaves behind.
        0 => {
            let cut = rng.below(bytes.len());
            bytes.truncate(cut);
        }
        // Flip one byte: a structural character becomes prose, or the reverse.
        1 => {
            let at = rng.below(bytes.len());
            if let Some(slot) = bytes.get_mut(at) {
                *slot = u8::try_from(rng.below(256)).unwrap();
            }
        }
        // Splice in a run of invalid UTF-8.
        2 => {
            let at = rng.below(bytes.len());
            bytes.splice(at..at, [0xFF, 0xFE, 0x80]);
        }
        // Repeat a slice, which unbalances braces and brackets.
        3 => {
            let at = rng.below(bytes.len());
            let run: Vec<u8> = bytes
                .get(at..)
                .unwrap_or_default()
                .iter()
                .copied()
                .take(8)
                .collect();
            bytes.splice(at..at, run);
        }
        // Delete a byte.
        4 => {
            let at = rng.below(bytes.len());
            bytes.remove(at);
        }
        // Deep nesting, which is where a recursive-descent parser blows a stack
        // if it has no depth guard.
        _ => {
            let depth = 1 + rng.below(96);
            let mut nested = vec![b'['; depth];
            nested.extend_from_slice(&bytes);
            nested.extend(std::iter::repeat_n(b']', depth));
            bytes = nested;
        }
    }
    bytes
}

/// No mutation of a valid document panics the parser.
///
/// The assertion is the absence of a crash: `from_slice` may answer `Ok` when a
/// mutation happened to stay legal, and may answer `Err` for every other, and
/// both are fine. What must never happen is an unwind or an abort, because a
/// peer controls this input and a panic in a request handler is that peer
/// choosing when the daemon stops serving.
#[test]
fn no_mutation_of_a_valid_document_panics_the_parser() {
    let mut rng = Seeded(0x5EED_1234_ABCD_9876);
    let mut parsed_anyway = 0_usize;

    for round in 0..MUTATION_ROUNDS {
        let Some(source) = SEED_CORPUS.get(round % SEED_CORPUS.len()) else {
            continue;
        };
        let probe = mutate(&mut rng, source);

        // Every runner-facing shape reads the same bytes: a mutation aimed at
        // one type is a perfectly good hostile payload for the others, and this
        // is the cheapest way to point all of them at it.
        if serde_json::from_slice::<SteerRequest<'_>>(&probe).is_ok() {
            parsed_anyway += 1;
        }
        if serde_json::from_slice::<SelftestCheck<'_>>(&probe).is_ok() {
            parsed_anyway += 1;
        }
        if serde_json::from_slice::<SelftestReport<'_>>(&probe).is_ok() {
            parsed_anyway += 1;
        }
    }

    // Not a bound on correctness — a guard on the corpus. If every mutation
    // were rejected at the first token the loop would prove nothing about the
    // field-walking code, and that is a corpus that has stopped doing its job.
    assert!(
        parsed_anyway > 0,
        "no mutation in {MUTATION_ROUNDS} rounds stayed parseable — the corpus \
         is being rejected at the first token and exercises nothing"
    );
}

/// Anything that does parse still answers to its declared bounds.
///
/// The pairing that matters: `serde` decides the SHAPE is legal and `garde`
/// decides the VALUES are, and a mutation that slips past the first has not
/// slipped past the second. Without this row a parser that accepted a
/// megabyte-long check name would look fine above.
#[test]
fn a_mutation_that_parses_is_still_held_to_its_bounds() {
    let mut rng = Seeded(0x0FF1_CE00_D15E_A5E5);

    for round in 0..MUTATION_ROUNDS {
        let Some(source) = SEED_CORPUS.get(round % SEED_CORPUS.len()) else {
            continue;
        };
        let probe = mutate(&mut rng, source);

        if let Ok(check) = serde_json::from_slice::<SelftestCheck<'_>>(&probe) {
            let within = check.name.len() <= CHECK_NAME_MAX_BYTES
                && !check.name.is_empty()
                && check.detail.len() <= CHECK_DETAIL_MAX_BYTES
                && !check.detail.is_empty();
            assert_eq!(
                check.validate().is_ok(),
                within,
                "a parsed check disagreed with its own bounds: {check:?}"
            );
        }

        if let Ok(steer) = serde_json::from_slice::<SteerRequest<'_>>(&probe) {
            let within = (1..=STEER_MESSAGE_MAX_BYTES).contains(&steer.message.len())
                && steer
                    .operation_id
                    .as_ref()
                    .is_none_or(|id| (1..=OPERATION_ID_MAX_BYTES).contains(&id.len()));
            assert_eq!(
                steer.validate().is_ok(),
                within,
                "a parsed steer disagreed with its own bounds: {steer:?}"
            );
        }
    }
}
