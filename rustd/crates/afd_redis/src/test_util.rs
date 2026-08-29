//! Bounded live-Redis connection setup for datastore-backed tests.
//!
//! A Rust test binary runs its tests in parallel. Opening one TLS connection
//! per test makes connection setup the bottleneck and can exhaust a short boot
//! deadline before Redis has accepted every handshake. Each `#[tokio::test]`
//! also owns a distinct runtime, so its `ConnectionManager` cannot outlive that
//! runtime and be shared process-wide. Serializing just the handshake keeps
//! every manager on its owning runtime without flooding the TLS listener.

use tokio::sync::Semaphore;

use crate::Redis;
use crate::config::RedisConfig;
use crate::error::Result;

static CONNECT_SERIAL: Semaphore = Semaphore::const_new(1);

/// Opens a live connection without competing with another test's handshake.
///
/// Fault-injection tests with private endpoints should keep using
/// [`Redis::connect`] or [`Redis::unreachable`] directly: only concurrent
/// connections to the lane's one TLS listener need this admission gate.
///
/// # Errors
/// Returns the connection attempt's configuration or transport failure.
pub async fn connect_live(config: &RedisConfig) -> Result<Redis> {
    // This private semaphore is never closed. Keeping the acquisition result
    // alive holds the permit for the handshake; `Err` is unconstructible while
    // the only code with access to the semaphore never calls `close`.
    let _permit = CONNECT_SERIAL.acquire().await;
    Redis::connect(config).await
}
