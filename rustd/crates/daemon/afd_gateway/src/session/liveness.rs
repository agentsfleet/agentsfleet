//! Only a valid host heartbeat extends the connection's liveness deadline.

use af_core::timing::HEARTBEAT_INTERVAL;
use parking_lot::Mutex;
use tokio::time::{Instant, sleep_until};

#[derive(Debug)]
pub(crate) struct Liveness {
    last_heartbeat: Mutex<Instant>,
}

impl Default for Liveness {
    fn default() -> Self {
        Self {
            last_heartbeat: Mutex::new(Instant::now()),
        }
    }
}

impl Liveness {
    pub(super) fn received(&self) {
        *self.last_heartbeat.lock() = Instant::now();
    }

    /// An intervening heartbeat moves the next wait; unrelated traffic never touches this clock.
    pub(super) async fn silence(&self) {
        loop {
            let deadline = *self.last_heartbeat.lock() + HEARTBEAT_INTERVAL * 2;
            sleep_until(deadline).await;
            if Instant::now() >= *self.last_heartbeat.lock() + HEARTBEAT_INTERVAL * 2 {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Liveness;
    use af_core::timing::HEARTBEAT_INTERVAL;
    use std::task::Poll;

    #[tokio::test(start_paused = true)]
    async fn a_session_without_heartbeats_closes_after_two_intervals() {
        let clock = Liveness::default();
        let silent = clock.silence();
        tokio::pin!(silent);
        assert!(matches!(futures_util::poll!(&mut silent), Poll::Pending));
        tokio::time::advance(HEARTBEAT_INTERVAL).await;
        assert!(matches!(futures_util::poll!(&mut silent), Poll::Pending));
        tokio::time::advance(HEARTBEAT_INTERVAL).await;
        assert!(matches!(futures_util::poll!(&mut silent), Poll::Ready(())));
    }

    #[tokio::test(start_paused = true)]
    async fn a_heartbeat_moves_an_already_waiting_silence_deadline() {
        let clock = Liveness::default();
        let silent = clock.silence();
        tokio::pin!(silent);
        assert!(matches!(futures_util::poll!(&mut silent), Poll::Pending));
        tokio::time::advance(HEARTBEAT_INTERVAL).await;
        clock.received();
        tokio::time::advance(HEARTBEAT_INTERVAL).await;
        assert!(matches!(futures_util::poll!(&mut silent), Poll::Pending));
        tokio::time::advance(HEARTBEAT_INTERVAL).await;
        assert!(matches!(futures_util::poll!(&mut silent), Poll::Ready(())));
    }
}
