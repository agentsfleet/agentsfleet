//! A failure is held until every task has ended.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]
#![expect(
    clippy::panic,
    reason = "a task that panics is the input one proof hands the join"
)]

use core::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::task::JoinHandle;

use super::all;
use crate::error::{ErrorKind, Result};

/// The role a lost task is reported under in these proofs.
const ROLE: &str = "test task";

/// Long enough that a join returning early would be seen to have.
const SLOW: Duration = Duration::from_millis(150);

fn refused() -> crate::error::Error {
    ErrorKind::TaskLost {
        role: "refusing task",
    }
    .into()
}

fn slow(finished: &Arc<AtomicBool>) -> JoinHandle<Result<u64>> {
    let finished = Arc::clone(finished);
    tokio::spawn(async move {
        tokio::time::sleep(SLOW).await;
        finished.store(true, Ordering::SeqCst);
        Ok(2)
    })
}

#[tokio::test]
async fn test_every_value_is_answered_in_task_order() {
    let tasks = vec![
        tokio::spawn(async { Ok::<u64, _>(1) }),
        tokio::spawn(async { Ok(2) }),
    ];
    assert_eq!(all(tasks, ROLE).await.expect("both succeed"), vec![1, 2]);
}

#[tokio::test]
async fn test_a_failure_waits_for_the_slower_task_to_end() {
    let finished = Arc::new(AtomicBool::new(false));
    let tasks = vec![tokio::spawn(async { Err(refused()) }), slow(&finished)];
    let failure = all(tasks, ROLE).await.expect_err("the first task refused");
    assert!(failure.to_string().contains("refusing task"), "{failure}");
    assert!(
        finished.load(Ordering::SeqCst),
        "the slow task ended before the failure was answered"
    );
}

#[tokio::test]
async fn test_the_first_failure_is_the_one_answered() {
    let tasks: Vec<JoinHandle<Result<u64>>> = vec![
        tokio::spawn(async { Err(refused()) }),
        tokio::spawn(async { Err(ErrorKind::TaskLost { role: "second" }.into()) }),
    ];
    let failure = all(tasks, ROLE).await.expect_err("both refused");
    assert!(failure.to_string().contains("refusing task"), "{failure}");
}

#[tokio::test]
async fn test_a_panicked_task_is_lost_under_its_role() {
    let tasks: Vec<JoinHandle<Result<u64>>> =
        vec![tokio::spawn(async { panic!("a window task panicked") })];
    let failure = all(tasks, ROLE).await.expect_err("a panic is a lost task");
    assert!(failure.to_string().contains(ROLE), "{failure}");
}
