//! What a settled report says about itself: the run's token sums. Split from
//! [`super`] the way `verdict/tests.rs` is, so the verb's own file stays readable.

use super::RunTokens;

const SAMPLE_INPUT: i64 = 1_000;

/// Every prompt class counts toward the run's input exactly once, and a
/// sum that would overflow saturates instead of wrapping.
#[test]
fn test_prompt_counts_reads_and_writes_once() {
    let tokens = RunTokens {
        input_tokens: SAMPLE_INPUT,
        cached_input_tokens: 400,
        cache_write_tokens: 250,
        output_tokens: 90,
    };
    assert_eq!(tokens.prompt(), 1_650, "output is not prompt");

    let full = RunTokens {
        input_tokens: i64::MAX,
        cache_write_tokens: 1,
        ..RunTokens::default()
    };
    assert_eq!(full.prompt(), i64::MAX);
}
