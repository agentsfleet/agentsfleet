//! One boundary binds private lease access to current connection authority and time.

use af_core::id::Uuid7;
use sqlx::{Postgres, Transaction};

use super::{Attempt, Counters, Guarded, Purpose};
use crate::{Drafts, Result, error::ErrorKind};

impl Drafts {
    /// Proves an invocation's private access and connection before work can leave the daemon.
    pub(crate) async fn invocation(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        runner: &Uuid7,
        lease: &Uuid7,
        counters: &Counters,
    ) -> Result<Guarded> {
        authorize(self, tx, runner, lease, counters, Purpose::Invocation).await
    }

    /// Proves current publication authority before reading a receipt or saving private output.
    pub(crate) async fn publication(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        runner: &Uuid7,
        lease: &Uuid7,
        counters: &Counters,
    ) -> Result<Guarded> {
        authorize(self, tx, runner, lease, counters, Purpose::Publication).await
    }

    /// Accounting retains its original identity after private access or lease expiry is gone.
    pub(crate) async fn accounting(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        runner: &Uuid7,
        lease: &Uuid7,
        counters: &Counters,
    ) -> Result<Attempt> {
        let attempt = super::accounting(tx, runner, lease, counters).await?;
        self.check_connection(tx, runner).await?;
        Ok(attempt)
    }
}

async fn authorize(
    drafts: &Drafts,
    tx: &mut Transaction<'_, Postgres>,
    runner: &Uuid7,
    lease: &Uuid7,
    counters: &Counters,
    purpose: Purpose,
) -> Result<Guarded> {
    let mut guarded = super::bound(tx, runner, lease, counters).await?;
    drafts.check_connection(tx, runner).await?;
    // The draft/attempt precedes the host lock; sample database time after that wait.
    guarded.now = super::refresh(tx, &guarded, counters).await?;
    if !purpose.accepts(&guarded.attempt) {
        return Err(ErrorKind::Fence.into());
    }
    if matches!(purpose, Purpose::Publication) {
        super::current_model(tx, &guarded).await?;
    }
    Ok(guarded)
}
