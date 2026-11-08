use std::time::{Duration, Instant};

use af_observability::producers::fleet::runner::stream::{self, NetworkDeltas};
use tokio::sync::watch;
use tokio_util::task::AbortOnDropHandle;

/// Live native counters sampled without calling an exporter from a QUIC callback.
#[derive(Clone, Copy, Debug, Default)]
pub struct NetworkStats {
    /// Round-trip time on the active path, when that path has a sample.
    pub rtt: Option<Duration>,
    /// Cumulative packets detected as lost on this connection.
    pub lost: u64,
    /// Cumulative packets sent on this connection.
    pub sent: u64,
    /// Cumulative sent packets carrying retransmitted data.
    pub retransmitted: u64,
    /// Cumulative probe timeouts across this connection's paths.
    pub probe_timeouts: u64,
    /// Cumulative retransmitted stream bytes on this connection.
    pub retransmitted_bytes: u64,
}

pub(super) struct Sampler {
    pub(super) sender: tokio::sync::watch::Sender<NetworkStats>,
    next: Instant,
}

impl Sampler {
    pub(super) fn new() -> (Self, tokio::sync::watch::Receiver<NetworkStats>) {
        let (sender, receiver) = tokio::sync::watch::channel(NetworkStats::default());
        (
            Self {
                sender,
                next: Instant::now(),
            },
            receiver,
        )
    }

    pub(super) fn sample(&mut self, connection: &tokio_quiche::quic::QuicheConnection) {
        let now = Instant::now();
        if now < self.next {
            return;
        }
        self.next = now + Duration::from_secs(1);
        self.finish(connection);
    }

    pub(super) fn finish(&mut self, connection: &tokio_quiche::quic::QuicheConnection) {
        let counters = connection.stats();
        let (rtt, probe_timeouts) =
            connection
                .path_stats()
                .fold((None, 0_u64), |(rtt, count), path| {
                    let rtt = if path.active { Some(path.rtt) } else { rtt };
                    (rtt, count.saturating_add(path.total_pto_count as u64))
                });
        self.sender.send_replace(NetworkStats {
            rtt,
            sent: counters.sent as u64,
            lost: counters.lost as u64,
            retransmitted: counters.retrans as u64,
            probe_timeouts,
            retransmitted_bytes: counters.stream_retrans_bytes,
        });
    }
}

/// A connection owns this task and joins it after the native driver is dropped.
pub(super) fn observe(mut samples: watch::Receiver<NetworkStats>) -> AbortOnDropHandle<()> {
    AbortOnDropHandle::new(tokio::spawn(async move {
        let mut previous = NetworkStats::default();
        loop {
            let current = *samples.borrow_and_update();
            stream::native(
                current.rtt,
                NetworkDeltas {
                    sent: current.sent.saturating_sub(previous.sent),
                    lost: current.lost.saturating_sub(previous.lost),
                    retransmitted: current.retransmitted.saturating_sub(previous.retransmitted),
                    probe_timeouts: current
                        .probe_timeouts
                        .saturating_sub(previous.probe_timeouts),
                    retransmitted_bytes: current
                        .retransmitted_bytes
                        .saturating_sub(previous.retransmitted_bytes),
                },
            );
            previous = current;
            if samples.changed().await.is_err() {
                return;
            }
        }
    }))
}
