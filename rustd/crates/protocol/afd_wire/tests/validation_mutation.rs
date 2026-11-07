//! What a malformed body does to the parser, over a generated corpus.
//!
//! # Why the input is generated
//!
//! Nobody can list the malformed documents a peer might send. What matters is
//! a single property that must hold for all of them: a bad body produces an
//! `Err`, never a panic, never an abort, never a half-built value. This file
//! throws several thousand mutations at the parser and asserts exactly that.
//! The bounds themselves are enumerable and tested exactly in `validation.rs`.
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
//! It is not a substitute for coverage-guided fuzzing. libFuzzer explores a
//! parser's branches and would find inputs a fixed corpus never reaches. If a
//! panic ever surfaces here it is real, and if one never does that is not proof
//! the parser is total. Adding libFuzzer means a nightly toolchain and an
//! out-of-band runner, so it cannot ride `make test-unit-all`; that remains
//! open.

#![expect(
    clippy::unwrap_used,
    reason = "test target: a value this file built wrong is an unmet \
              precondition, and failing loudly on it is the correct outcome"
)]

use afd_wire::event::{OPERATION_ID_MAX_BYTES, STEER_MESSAGE_MAX_BYTES, SteerRequest};
use afd_wire::runner::{
    CHECK_DETAIL_MAX_BYTES, CHECK_NAME_MAX_BYTES, SelftestCheck, SelftestReport,
};
use garde::Validate as _;

/// How many mutations the parser survives per seed corpus entry.
///
/// Sized to run in well under a second while still exercising every mutation
/// arm many times over. Raising it is a review-able diff, which is the point.
const MUTATION_ROUNDS: usize = 4_000;

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
                && !steer.message.contains('\0')
                && steer.operation_id.as_ref().is_none_or(|id| {
                    (1..=OPERATION_ID_MAX_BYTES).contains(&id.len()) && !id.contains('\0')
                });
            assert_eq!(
                steer.validate().is_ok(),
                within,
                "a parsed steer disagreed with its own bounds: {steer:?}"
            );
        }
    }
}
