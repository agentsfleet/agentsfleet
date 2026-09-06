//! What the counter parsers read, and what they refuse to invent.

use super::redis_calls_in;

/// Two commands' worth of `INFO commandstats`, as Redis prints it.
const COMMANDSTATS: &str = "# Commandstats\r\n\
cmdstat_hset:calls=12,usec=48,usec_per_call=4.00,rejected_calls=0,failed_calls=0\r\n\
cmdstat_xadd:calls=30,usec=900,usec_per_call=30.00,rejected_calls=0,failed_calls=0\r\n";

#[test]
fn test_redis_calls_sum_every_command_family() {
    assert_eq!(redis_calls_in(COMMANDSTATS), Some(42));
}

#[test]
fn test_a_reply_with_no_call_counts_is_refused_rather_than_read_as_zero() {
    assert_eq!(
        redis_calls_in("# Commandstats\r\n"),
        None,
        "a server that answered nothing this parser knows did not serve zero commands"
    );
    assert_eq!(redis_calls_in(""), None);
}

#[test]
fn test_a_malformed_count_is_skipped_and_the_rest_still_read() {
    let mixed = "cmdstat_get:calls=abc,usec=1\r\ncmdstat_set:calls=7,usec=1\r\n";
    assert_eq!(redis_calls_in(mixed), Some(7));
}
