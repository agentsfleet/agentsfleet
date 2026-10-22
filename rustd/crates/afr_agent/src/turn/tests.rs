//! Known usage survives failure or cancellation, without authorizing a tool call.
#![expect(clippy::expect_used, reason = "fixture preconditions must fail loudly")]

use super::*;
use afr_secrets::Scrub;
use futures_util::stream;
use std::time::Instant;

const SPENT: Usage = Usage {
    input: 6,
    cached_input: 4,
    output: 3,
    cache_write: 0,
    cache_write_5m: 0,
    cache_write_1h: 0,
};

#[tokio::test]
async fn usage_is_recorded_once_even_when_the_next_chunk_fails() {
    let scrub = Scrub::of(std::iter::empty()).expect("empty scrub");
    let mut live = Live::silent(&scrub, Instant::now());
    let meter = Meter::default();
    let source = stream::iter([
        Ok(Chunk::Usage(SPENT)),
        Err(afr_providers::Error::lost(std::io::Error::other("reset"))),
    ])
    .boxed();
    let error = take(source, &mut live, &meter)
        .await
        .expect_err("the next chunk failed");
    assert_eq!(
        error.code(),
        afd_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert_eq!(meter.read(), SPENT);
    assert_eq!(meter.backlog(), 1);
}

#[tokio::test]
async fn a_successful_turn_does_not_record_usage_twice() {
    let scrub = Scrub::of(std::iter::empty()).expect("empty scrub");
    let mut live = Live::silent(&scrub, Instant::now());
    let meter = Meter::default();
    let source = stream::iter([
        Ok(Chunk::Usage(SPENT)),
        Ok(Chunk::End(afr_providers::End {
            replay: Replay::default(),
            cut: false,
        })),
    ])
    .boxed();
    assert_eq!(
        take(source, &mut live, &meter)
            .await
            .expect("completed turn")
            .usage,
        SPENT
    );
    assert_eq!(meter.read(), SPENT);
    assert_eq!(meter.backlog(), 1);
}

#[tokio::test]
async fn dropping_the_consumer_after_usage_keeps_the_request_line() {
    let scrub = Scrub::of(std::iter::empty()).expect("empty scrub");
    let mut live = Live::silent(&scrub, Instant::now());
    let meter = Meter::default();
    let source = stream::iter([Ok(Chunk::Usage(SPENT))])
        .chain(stream::pending())
        .boxed();
    let mut reading = Box::pin(take(source, &mut live, &meter));
    assert!(futures_util::poll!(&mut reading).is_pending());
    drop(reading);
    assert_eq!(meter.read(), SPENT);
    assert_eq!(meter.backlog(), 1);
}
