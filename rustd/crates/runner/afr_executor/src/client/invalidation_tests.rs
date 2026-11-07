//! One lost process link fences RPCs and every process sharing that client.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

use std::time::Duration;

use afd_core::error_code;
use futures_util::FutureExt as _;
use serde_json::Value;
use tokio::io::AsyncBufReadExt as _;

use super::tests::{answer_spawn, next_call, started};
use crate::{Ending, Executor as _, Process, ProcessEvent, Spawn};

const PATIENCE: Duration = Duration::from_secs(2);

// Value: protects=link loss ends pending work and refuses later work without another send;
// fails_when=disconnect only drops one reader, or subsequent calls queue on the lost link;
// seam=real native Client over a socket peer; no_claim=remote process death.
#[tokio::test]
async fn invalidating_one_process_interrupts_the_shared_executor_link() {
    let (client, mut peer, mut first) = started().await;
    let spawn = Spawn::program("sleep");
    let (second, ()) = tokio::join!(client.spawn(&spawn), answer_spawn(&mut peer, 8));
    let mut second = second.expect("second attached process");
    let invalidate = async {
        let request = tokio::time::timeout(PATIENCE, next_call(&mut peer))
            .await
            .expect("pending read sent");
        assert_eq!(
            request.get("method").and_then(Value::as_str),
            Some("fs/read")
        );
        first.events.disconnect();
        // This runs before yielding to the background link task.
        let refused = client
            .list_dir(".")
            .now_or_never()
            .expect("lost link refuses in the initial poll")
            .expect_err("refused");
        assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
        assert!(
            refused
                .to_string()
                .contains("the executor connection closed")
        );
    };
    let (pending, ()) = tokio::join!(client.read_file("pending", 1), invalidate);
    let failure = pending.expect_err("pending read interrupted");
    assert_eq!(failure.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(
        failure
            .to_string()
            .contains("the executor connection closed")
    );
    assert_interrupted(&mut first).await;
    assert_interrupted(&mut second).await;
    let mut trailing = String::new();
    let bytes = tokio::time::timeout(PATIENCE, peer.read_line(&mut trailing))
        .await
        .expect("socket closes")
        .expect("peer closure");
    assert_eq!(
        (bytes, trailing.as_str()),
        (0, ""),
        "no later RPC or futile kill"
    );
}

async fn assert_interrupted(process: &mut Process) {
    let ending = tokio::time::timeout(PATIENCE, process.events.recv())
        .await
        .expect("process interrupted promptly");
    assert_eq!(
        ending,
        Some(ProcessEvent::Ended {
            ending: Ending::Interrupted,
            output_abandoned: false
        })
    );
    assert_eq!(process.events.recv().await, None, "one interruption only");
}
