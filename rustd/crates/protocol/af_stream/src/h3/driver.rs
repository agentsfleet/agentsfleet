use tokio::sync::{oneshot, watch};
use tokio_quiche::{
    ApplicationOverQuic, QuicResult,
    metrics::Metrics,
    quic::{HandshakeInfo, QuicheConnection},
};
use tokio_util::sync::CancellationToken;

use super::telemetry::{NetworkStats, Sampler};

pub(super) struct Driver<A> {
    inner: A,
    stop: CancellationToken,
    done: Option<oneshot::Sender<()>>,
    sample: Sampler,
    security: watch::Sender<Option<crate::tls::KeyExchange>>,
}

impl<A> Driver<A> {
    pub(super) fn new(
        inner: A,
        stop: CancellationToken,
    ) -> (
        Self,
        oneshot::Receiver<()>,
        watch::Receiver<NetworkStats>,
        watch::Receiver<Option<crate::tls::KeyExchange>>,
    ) {
        let (done, completion) = oneshot::channel();
        let (sample, stats) = Sampler::new();
        let (security, negotiated) = watch::channel(None);
        (
            Self {
                inner,
                stop,
                done: Some(done),
                sample,
                security,
            },
            completion,
            stats,
            negotiated,
        )
    }
}

impl<A: ApplicationOverQuic> ApplicationOverQuic for Driver<A> {
    fn on_conn_established(
        &mut self,
        qconn: &mut QuicheConnection,
        handshake_info: &HandshakeInfo,
    ) -> QuicResult<()> {
        let ssl: &mut boring::ssl::SslRef = qconn.as_mut();
        let group = crate::tls::KeyExchange::from(&*ssl);
        self.security.send_replace(Some(group));
        group.check()?;
        self.sample.sample(qconn);
        self.inner.on_conn_established(qconn, handshake_info)
    }

    fn should_act(&self) -> bool {
        self.inner.should_act() || self.stop.is_cancelled()
    }

    async fn wait_for_data(&mut self, qconn: &mut QuicheConnection) -> QuicResult<()> {
        tokio::select! {
            biased;
            () = self.stop.cancelled() => Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into()),
            result = self.inner.wait_for_data(qconn) => result,
        }
    }

    fn process_reads(&mut self, qconn: &mut QuicheConnection) -> QuicResult<()> {
        self.sample.sample(qconn);
        self.inner.process_reads(qconn)
    }

    fn process_writes(&mut self, qconn: &mut QuicheConnection) -> QuicResult<()> {
        self.sample.sample(qconn);
        if self.stop.is_cancelled() {
            let _already_closed = qconn.close(true, 0, b"");
            return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into());
        }
        self.inner.process_writes(qconn)
    }

    fn on_conn_close<M: Metrics>(
        &mut self,
        qconn: &mut QuicheConnection,
        metrics: &M,
        connection_result: &QuicResult<()>,
    ) {
        self.sample.finish(qconn);
        self.inner.on_conn_close(qconn, metrics, connection_result);
    }
}

impl<A> Drop for Driver<A> {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(done) = self.done.take() {
            let _owner_gone = done.send(());
        }
    }
}
