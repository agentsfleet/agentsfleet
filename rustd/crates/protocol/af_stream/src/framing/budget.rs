//! Separate pools preserve control memory when data traffic saturates.

use std::sync::Arc;

use af_wire::protocol::StreamKind;
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
    /// Independent receive and send pools prevent held input from starving its own answer.
    /// Each direction receives half of the validated deployment budget.
    #[must_use]
    pub fn duplex(limits: super::MemoryLimits) -> (Self, Self) {
        let half = || Self {
            control: Arc::new(Semaphore::new(limits.control_bytes / 2)),
            data: Arc::new(Semaphore::new(limits.data_bytes / 2)),
        };
        (half(), half())
    }

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
    pub fn available(&self, kind: StreamKind) -> usize {
        self.pool(kind).available_permits()
    }

    fn pool(&self, kind: StreamKind) -> &Arc<Semaphore> {
        match kind {
            StreamKind::Control => &self.control,
            StreamKind::Work | StreamKind::Reports | StreamKind::Activity => &self.data,
        }
    }

    pub(super) fn try_reserve(
        &self,
        kind: StreamKind,
        bytes: usize,
    ) -> Result<OwnedSemaphorePermit> {
        let requested = u32::try_from(bytes)?;
        Ok(Arc::clone(self.pool(kind)).try_acquire_many_owned(requested)?)
    }

    pub(super) async fn reserve(
        &self,
        kind: StreamKind,
        bytes: usize,
        stop: &CancellationToken,
        deadline: Instant,
    ) -> Result<OwnedSemaphorePermit> {
        let requested = u32::try_from(bytes)?;
        let pool = Arc::clone(self.pool(kind));
        error::within(stop, deadline, Stage::Memory, async move {
            Ok(pool.acquire_many_owned(requested).await?)
        })
        .await
    }
}
