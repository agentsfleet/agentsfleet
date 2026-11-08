//! Separate pools preserve control memory when data traffic saturates.

use std::sync::Arc;

use af_wire::protocol::Lane;
use garde::{Unvalidated, Validate};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::{Result, Stage, error};

/// Replica-wide frame memory, shared by encoders and decoders.
#[derive(Debug, Clone)]
pub struct Budget {
    control: Arc<Semaphore>,
    data: Arc<Semaphore>,
}

#[derive(Validate)]
struct Pools {
    #[garde(range(max = Semaphore::MAX_PERMITS))]
    control: usize,
    #[garde(range(max = Semaphore::MAX_PERMITS))]
    data: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            control: Arc::new(Semaphore::new(16 * 1024 * 1024)),
            data: Arc::new(Semaphore::new(48 * 1024 * 1024)),
        }
    }
}

impl Budget {
    /// Allocate separate byte pools; small pools also support pressure tests.
    ///
    /// # Errors
    /// Refuses a pool larger than Tokio's supported semaphore capacity.
    pub fn new(control_bytes: usize, data_bytes: usize) -> Result<Self> {
        let pools = Unvalidated::new(Pools {
            control: control_bytes,
            data: data_bytes,
        })
        .validate()?;
        Ok(Self {
            control: Arc::new(Semaphore::new(pools.control)),
            data: Arc::new(Semaphore::new(pools.data)),
        })
    }

    /// Bytes available without waiting, used by queue-pressure telemetry.
    #[must_use]
    pub fn available(&self, lane: Lane) -> usize {
        self.pool(lane).available_permits()
    }

    fn pool(&self, lane: Lane) -> &Arc<Semaphore> {
        match lane {
            Lane::Control => &self.control,
            Lane::Work | Lane::Reports | Lane::Activity => &self.data,
        }
    }

    pub(super) async fn reserve(
        &self,
        lane: Lane,
        bytes: usize,
        stop: &CancellationToken,
        deadline: Instant,
    ) -> Result<OwnedSemaphorePermit> {
        let requested = u32::try_from(bytes)?;
        let pool = Arc::clone(self.pool(lane));
        error::within(stop, deadline, Stage::Memory, async move {
            Ok(pool.acquire_many_owned(requested).await?)
        })
        .await
    }
}
