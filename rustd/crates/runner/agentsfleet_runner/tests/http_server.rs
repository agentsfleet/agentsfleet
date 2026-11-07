//! The fake daemon and collector's loopback listener, shared by kernel tests.
#![allow(
    dead_code,
    reason = "shared test support: each suite uses a subset of the lifecycle methods"
)]

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Binds once, retains the listener, and aborts it when its owner drops.
pub(crate) struct Server {
    pub(crate) url: String,
    stop: CancellationToken,
    task: JoinHandle<std::io::Result<()>>,
}

impl Server {
    pub(crate) async fn serve(app: axum::Router) -> std::io::Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let stop = CancellationToken::new();
        let stopped = stop.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(stopped.cancelled_owned())
                .await
        });
        Ok(Self { url, stop, task })
    }

    pub(crate) async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.stop.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(10), &mut self.task).await???;
        Ok(())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.cancel();
        self.task.abort();
    }
}
