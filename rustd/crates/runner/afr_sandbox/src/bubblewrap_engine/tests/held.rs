//! A blocking step's value comes back to the parts that keep it, whatever the
//! step does.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::bubblewrap_engine::held::off_runtime;

/// Records its own drop, as an egress scope's claim frees its slot on drop.
#[derive(Debug)]
struct Freed(Arc<AtomicBool>);

impl Drop for Freed {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// A step that returns hands back what it changed; one that panics fails as
/// the pool's task failure and leaves its value held, never dropped inside
/// the task; nothing held runs nothing.
#[tokio::test]
#[expect(
    clippy::panic,
    reason = "the step under test panics, as a bug in a scope's swap would"
)]
async fn test_a_held_value_comes_back_even_when_its_step_panics() {
    let freed = Arc::new(AtomicBool::new(false));
    let mut held = Some((Freed(Arc::clone(&freed)), 1));

    let stepped = off_runtime(&mut held, |(_freed, count)| {
        *count += 1;
        *count
    })
    .await;
    let panicked = off_runtime(&mut held, |(_freed, count)| {
        *count += 1;
        panic!("the swap failed after {count} steps");
    })
    .await;
    let mut empty: Option<u8> = None;
    let nothing = off_runtime(&mut empty, |value| *value).await;

    assert_eq!(stepped.ok().flatten(), Some(2));
    let told = panicked.err().map(|failure| failure.to_string());
    assert!(
        told.as_deref()
            .is_some_and(|told| told.contains("a task did not finish")),
        "{told:?}"
    );
    assert_eq!(held.as_ref().map(|(_freed, count)| *count), Some(3));
    assert!(!freed.load(Ordering::SeqCst), "the value was never dropped");
    assert!(matches!(nothing, Ok(None)));
    drop(held);
    assert!(freed.load(Ordering::SeqCst));
}
