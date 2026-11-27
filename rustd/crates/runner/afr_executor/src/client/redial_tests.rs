//! A link a park closed is dialled again, once, by the next call.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

use std::io;
use std::sync::Mutex;
use std::time::Duration;

use tokio::io::{BufReader, DuplexStream};

use super::Client;
use super::route::{ExecutorRoute, ExecutorStream};
use super::tests::{answer, answer_spawn, nothing_listed};
use crate::{Ending, Executor as _, ProcessEvent, Sizes, Spawn};

/// How long a test waits for anything that should be prompt.
const PATIENCE: Duration = Duration::from_secs(10);
/// Bytes each in-memory stream buffers: more than any line here.
const DUPLEX_BYTES: usize = 64 * 1024;

/// A route handing out prepared streams in order, then refusing.
#[derive(Debug)]
struct Prepared(Mutex<Vec<DuplexStream>>);

#[async_trait::async_trait]
impl ExecutorRoute for Prepared {
    async fn dial(&self) -> io::Result<Box<dyn ExecutorStream>> {
        let mut left = self.0.lock().expect("no dial panicked");
        let next = (!left.is_empty()).then(|| left.remove(0));
        next.map(|stream| Box::new(stream) as Box<dyn ExecutorStream>)
            .ok_or_else(|| io::Error::from(io::ErrorKind::ConnectionRefused))
    }
}

// Value: protects=a parked VM's executor link comes back for the next call, and work the old
// link carried is never reported finished; fails_when=a closed link stays closed, the client
// redials more than once, or the old process reads anything but interrupted;
// seam=real Client over in-memory streams; no_claim=the executor resuming the old process.
#[tokio::test]
async fn test_the_executor_link_redials_after_a_park() {
    let (first, first_far) = tokio::io::duplex(DUPLEX_BYTES);
    let (second, second_far) = tokio::io::duplex(DUPLEX_BYTES);
    let client = Client::dial(Prepared(Mutex::new(vec![first, second])), Sizes::DEFAULT)
        .await
        .expect("the first dial answers");
    let mut far = BufReader::new(first_far);
    let sleep = Spawn::program("sleep");
    let (spawned, ()) = tokio::join!(client.spawn(&sleep), answer_spawn(&mut far, 1));
    let mut process = spawned.expect("the first link started a process");

    // The VM parks: its end of the first link goes away.
    drop(far);
    let ended = tokio::time::timeout(PATIENCE, process.events.recv())
        .await
        .expect("the old link's process ended promptly");
    let mut far = BufReader::new(second_far);
    let (listed, call) = tokio::join!(client.list_dir("."), answer(&mut far, nothing_listed()));
    drop(far);
    let after = tokio::time::timeout(PATIENCE, client.list_dir("."))
        .await
        .expect("a refused dial fails promptly");

    assert_eq!(
        ended,
        Some(ProcessEvent::Ended {
            ending: Ending::Interrupted,
            output_abandoned: false
        })
    );
    assert_eq!(
        process.events.recv().await,
        None,
        "interrupted exactly once"
    );
    assert_eq!(
        call.get("method").and_then(serde_json::Value::as_str),
        Some(crate::protocol::METHOD_LIST_DIR)
    );
    assert_eq!(
        listed.expect("the redialled link answers").entries,
        Vec::new()
    );
    let refused = after.expect_err("a second closed link fails the call");
    assert!(
        refused
            .to_string()
            .contains("the executor connection closed"),
        "{refused}"
    );
}
