#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;

use super::{Memory, Settings};
use crate::{Consumer, Offer, Outcome, Sink};

/// A producer and its message index.
type Message = (u32, u32);

const NO_RETRY: &[Duration] = &[];
const TWO_RETRIES: &[Duration] = &[Duration::from_millis(1), Duration::from_millis(1)];

/// Records what it consumed; can hold each message until opened, or fail a
/// message a given number of times first.
#[derive(Debug, Default)]
struct Recorder {
    seen: Mutex<Vec<(u32, u32)>>,
    gate: Option<Arc<Notify>>,
    failures_left: Mutex<u32>,
    abandoned: Mutex<Vec<(u32, u32)>>,
}

impl Recorder {
    fn seen(&self) -> Vec<(u32, u32)> {
        self.seen.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Consumer<(u32, u32)> for Recorder {
    async fn consume(&self, message: &(u32, u32)) -> Outcome {
        if let Some(gate) = &self.gate {
            gate.notified().await;
        }
        {
            let mut left = self.failures_left.lock().unwrap();
            if *left > 0 {
                *left -= 1;
                return Outcome::Retry;
            }
        }
        self.seen.lock().unwrap().push(*message);
        Outcome::Done
    }

    fn abandoned(&self, message: &(u32, u32)) {
        self.abandoned.lock().unwrap().push(*message);
    }
}

fn settings(depth: usize, retry: &'static [Duration]) -> Settings {
    Settings {
        depth,
        workers: 1,
        retry,
    }
}

/// Routes by producer, so one producer's messages share a worker.
fn by_producer() -> super::Shard<Message> {
    Arc::new(|message: &Message| u64::from(message.0))
}

async fn until(recorder: &Recorder, count: usize) {
    for _ in 0..1_000 {
        if recorder.seen().len() + recorder.abandoned.lock().unwrap().len() >= count {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("only {} of {count} messages arrived", recorder.seen().len());
}

#[tokio::test]
async fn one_producers_messages_arrive_once_and_in_order() {
    let recorder = Arc::new(Recorder::default());
    let queue = Memory::start(
        &(Arc::clone(&recorder) as Arc<dyn Consumer<Message>>),
        settings(2_000, NO_RETRY),
        by_producer(),
    );

    for index in 0..1_000 {
        assert_eq!(queue.offer((0, index)), Offer::Accepted);
    }

    until(&recorder, 1_000).await;
    assert_eq!(
        recorder.seen(),
        (0..1_000).map(|index| (0, index)).collect::<Vec<_>>()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn racing_producers_lose_and_duplicate_nothing_and_keep_their_own_order() {
    const PRODUCERS: u32 = 32;
    const EACH: u32 = 500;
    let recorder = Arc::new(Recorder::default());
    let queue = Arc::new(Memory::start(
        &(Arc::clone(&recorder) as Arc<dyn Consumer<Message>>),
        Settings {
            depth: 256,
            workers: 4,
            retry: NO_RETRY,
        },
        by_producer(),
    ));
    let start = Arc::new(tokio::sync::Barrier::new(PRODUCERS as usize));

    // Spawned before any is awaited: a lazy map would leave the barrier one short.
    let producers = (0..PRODUCERS)
        .map(|producer| {
            let queue = Arc::clone(&queue);
            let start = Arc::clone(&start);
            tokio::spawn(async move {
                start.wait().await;
                let mut refused = Vec::new();
                for index in 0..EACH {
                    if let Offer::Refused(message) = queue.offer((producer, index)) {
                        refused.push(message);
                    }
                }
                refused
            })
        })
        .collect::<Vec<_>>();
    let mut refused = Vec::new();
    for producer in producers {
        refused.extend(producer.await.unwrap());
    }
    let accepted = (PRODUCERS * EACH) as usize - refused.len();

    until(&recorder, accepted).await;
    let seen = recorder.seen();
    assert_eq!(
        seen.len(),
        accepted,
        "every accepted message arrived, and only those"
    );
    let mut last: HashMap<u32, u32> = HashMap::new();
    for (producer, index) in &seen {
        if let Some(previous) = last.insert(*producer, *index) {
            assert!(
                previous < *index,
                "producer {producer} went {previous} then {index}"
            );
        }
        assert!(
            !refused.contains(&(*producer, *index)),
            "a refused message was also delivered"
        );
    }
}

#[tokio::test]
async fn a_full_queue_refuses_and_hands_the_message_back() {
    let gate = Arc::new(Notify::new());
    let recorder = Arc::new(Recorder {
        gate: Some(Arc::clone(&gate)),
        ..Recorder::default()
    });
    let queue = Memory::start(
        &(Arc::clone(&recorder) as Arc<dyn Consumer<Message>>),
        settings(1, NO_RETRY),
        by_producer(),
    );

    assert_eq!(queue.offer((0, 0)), Offer::Accepted);
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(
        queue.offer((0, 1)),
        Offer::Accepted,
        "one waits in the channel"
    );
    assert_eq!(queue.offer((0, 2)), Offer::Refused((0, 2)));

    gate.notify_one();
    tokio::time::sleep(Duration::from_millis(20)).await;
    gate.notify_one();
    until(&recorder, 2).await;
    assert_eq!(recorder.seen(), [(0, 0), (0, 1)]);
}

#[tokio::test]
async fn a_retried_message_is_handled_once_it_succeeds() {
    let recorder = Arc::new(Recorder {
        failures_left: Mutex::new(2),
        ..Recorder::default()
    });
    let queue = Memory::start(
        &(Arc::clone(&recorder) as Arc<dyn Consumer<Message>>),
        settings(8, TWO_RETRIES),
        by_producer(),
    );

    queue.offer((0, 7));

    until(&recorder, 1).await;
    assert_eq!(recorder.seen(), [(0, 7)]);
    assert_eq!(*recorder.abandoned.lock().unwrap(), Vec::<Message>::new());
}

#[tokio::test]
async fn a_message_that_outlasts_the_ladder_is_abandoned_once() {
    let recorder = Arc::new(Recorder {
        failures_left: Mutex::new(3),
        ..Recorder::default()
    });
    let queue = Memory::start(
        &(Arc::clone(&recorder) as Arc<dyn Consumer<Message>>),
        settings(8, TWO_RETRIES),
        by_producer(),
    );

    queue.offer((0, 9));

    until(&recorder, 1).await;
    assert!(
        recorder.seen().is_empty(),
        "three failures exceed two retries"
    );
    assert_eq!(*recorder.abandoned.lock().unwrap(), [(0, 9)]);
}

#[test]
fn without_a_runtime_every_offer_is_refused_so_the_producer_does_the_work() {
    let recorder = Arc::new(Recorder::default());
    let queue = Memory::start(
        &(Arc::clone(&recorder) as Arc<dyn Consumer<Message>>),
        settings(8, NO_RETRY),
        by_producer(),
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    let offered = runtime.block_on(async { queue.offer((0, 1)) });

    assert_eq!(offered, Offer::Refused((0, 1)));
    assert_eq!(recorder.seen(), Vec::<Message>::new());
}

/// Keeps the trait's default `abandoned`: retries message `(0, 1)` forever and
/// records every other.
#[derive(Debug, Default)]
struct RetriesFirst {
    seen: Mutex<Vec<Message>>,
}

#[async_trait::async_trait]
impl Consumer<Message> for RetriesFirst {
    async fn consume(&self, message: &Message) -> Outcome {
        if *message == (0, 1) {
            return Outcome::Retry;
        }
        self.seen.lock().unwrap().push(*message);
        Outcome::Done
    }
}

#[tokio::test]
async fn after_the_default_abandon_hook_the_same_queue_delivers_the_next_message() {
    let consumer = Arc::new(RetriesFirst::default());
    let queue = Memory::start(
        &(Arc::clone(&consumer) as Arc<dyn Consumer<Message>>),
        settings(8, TWO_RETRIES),
        by_producer(),
    );

    assert_eq!(queue.offer((0, 1)), Offer::Accepted);
    assert_eq!(queue.offer((0, 2)), Offer::Accepted);

    for _ in 0..1_000 {
        if !consumer.seen.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        *consumer.seen.lock().unwrap(),
        [(0, 2)],
        "the second message follows the abandoned first"
    );
}

#[tokio::test]
async fn its_debug_form_names_the_worker_count() {
    let recorder = Arc::new(Recorder::default());
    let queue = Memory::start(
        &(Arc::clone(&recorder) as Arc<dyn Consumer<Message>>),
        Settings {
            depth: 4,
            workers: 3,
            retry: NO_RETRY,
        },
        by_producer(),
    );

    assert_eq!(format!("{queue:?}"), "Memory { workers: 3, .. }");
}
