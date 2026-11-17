//! The runner store: one pool, one entropy source, and the verbs over both.

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_db::Db;
use jiff::Timestamp;
use sqlx::Acquire as _;

use crate::error::{Result, query};
use crate::reconcile::Verdict;
use crate::register::Prepared;
use crate::scope::Owner;
use af_wire::runner::RegisterRequest;
use afd_auth::credential::CredentialKind;

use afd_auth::minted::Minted;

/// Statement names, for the context a query failure carries.
///
/// Named because each is spelled at a `map_err` and would otherwise be an
/// inline literal at every call site (RULE UFS), and because the string reaches
/// an operator's log as the answer to "which statement".
pub(crate) const CONTEXT_REGISTER: &str = "runner enrolment";

/// What enrolment produced.
///
/// The token is revealed exactly once — in the response to the request that
/// minted it — so it is returned rather than stored anywhere, and its own
/// `Drop` zeroes it when that response has been written.
#[derive(Debug)]
pub struct Enrolled {
    /// The runner's durable identifier.
    pub runner_id: Uuid7,
    /// The bearer token, revealed once.
    pub token: Minted,
    /// The worker ceiling as CLAMPED and stored, so the caller echoes what the
    /// host will actually apply rather than what was asked for.
    pub worker_count: u32,
    /// The verdict the row opens with.
    pub verdict: Verdict,
}

/// Runner-plane reads and writes, over the api-role pool.
///
/// Holds the pool and the entropy source together because enrolment needs both
/// and neither is meaningful here alone. Cheap to clone: `Db` is a handle and
/// `Entropy` is a zero-sized selector over the system source.
#[derive(Debug, Clone)]
pub struct Runners {
    database: Db,
    entropy: Entropy,
}

impl Runners {
    /// A store reading and writing through `database`.
    #[must_use]
    pub const fn new(database: Db, entropy: Entropy) -> Self {
        Self { database, entropy }
    }

    /// The pool this store reads through, for the sibling modules that add
    /// verbs to [`Runners`] in their own files.
    ///
    /// `pub(crate)`, not `pub`: the pool is an implementation detail of this
    /// crate, and handing it out would let a caller run a statement that is not
    /// in `sql/` — which is the property that makes the side-by-side parity
    /// read of that module meaningful.
    pub(crate) const fn pool(&self) -> &Db {
        &self.database
    }

    /// The entropy source, for the sibling modules that mint an identifier.
    ///
    /// `pub(crate)` for the same reason [`Runners::pool`] is: the source is an
    /// implementation detail, and handing it out would let a caller draw a
    /// credential through a path this crate cannot see.
    pub(crate) const fn entropy(&self) -> &Entropy {
        &self.entropy
    }

    /// Draws the two identifiers and the credential an enrolment needs.
    ///
    /// Split out because it is the only part that touches entropy, and both
    /// registration entries draw through it in `Prepared::of`.
    pub(crate) fn mint(&self, now: Timestamp) -> Result<(Uuid7, Uuid7, Minted)> {
        let runner_id = self.entropy.uuid7(now)?;
        let event_id = self.entropy.uuid7(now)?;
        Ok((
            runner_id,
            event_id,
            Minted::draw(CredentialKind::RunnerToken, &self.entropy)?,
        ))
    }

    /// Enrols a runner, storing only the digest of the token it returns, and
    /// each of its labels once.
    ///
    /// The row and its enrolment event land in ONE statement, so an observer
    /// can never see a registered runner with no audit row explaining where it
    /// came from. `now` mints the two identifiers; the database stamps the row.
    ///
    /// # The initial verdict is reconciled against NO report
    ///
    /// An assignment that demands enforcement therefore opens `degraded` with
    /// "no capability report", and the lease gate refuses work until the host's
    /// first heartbeat proves the cage. That is deliberate: the alternative is
    /// a fail-OPEN window between minting a token and the first beat.
    ///
    /// # Errors
    /// Refuses a `host_id` outside its bounds, labels past theirs, or a
    /// malformed assignment; reports a datastore that would not answer, and an
    /// entropy source that could not produce a credential.
    pub async fn register(
        &self,
        request: &RegisterRequest<'_>,
        now: Timestamp,
    ) -> Result<Enrolled> {
        let prepared = Prepared::of(self, request, now)?;
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_REGISTER))?;
        prepared.insert(&mut transaction, Owner::Platform).await?;
        transaction
            .commit()
            .await
            .map_err(query(CONTEXT_REGISTER))?;
        Ok(prepared.enrolled())
    }
}

pub(crate) fn record_enrolled(runner: &Uuid7, host: &str, verdict: Verdict) {
    let runner_id = runner.as_str();
    let degraded = verdict.is_degraded();
    {
        let (host_id, event, message) =
            (&host, &"runner_enrolled", format_args!("runner enrolled"));
        tracing::debug!(message, runner_id, host_id, degraded, event);
    };
}
