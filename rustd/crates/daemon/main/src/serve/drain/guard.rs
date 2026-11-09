//! One connection releases its drain count through Drop on every exit path.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::Notify;

/// One connection's claim on the drain, released by dropping it.
///
/// `Drop` rather than an explicit `release`, because the connection task can end
/// by returning, by an error, or by being cancelled mid-await, and only `Drop`
/// covers all three. A count that leaked on the cancelled path would make every
/// drain after the first one wait out its whole bound.
#[derive(Debug)]
pub struct Guard {
    pub(super) live: Arc<AtomicUsize>,
    pub(super) idle: Arc<Notify>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        // `fetch_sub` returns the PREVIOUS value, so 1 means this guard was the
        // last one. Notifying only then keeps a busy server from waking the
        // waiter on every completed request.
        if self.live.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.idle.notify_waiters();
        }
    }
}
