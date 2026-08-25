//! The runner store: one pool, one entropy source, and the verbs over both.

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_wire::runner::{AssignedPolicy, RegisterRequest};

use crate::error::{Result, query};
use crate::runner::reconcile::{Verdict, reconcile};
use crate::runner::spelling::{policy_wire, render_list, tier_wire};
use crate::runner::token::Minted;
use crate::runner::validate::{HostId, assignment};
use crate::sql;

/// Statement names, for the context a query failure carries.
///
/// Named because each is spelled at a `map_err` and would otherwise be an
/// inline literal at every call site (RULE UFS), and because the string reaches
/// an operator's log as the answer to "which statement".
const CONTEXT_REGISTER: &str = "runner enrolment";

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
    /// Split out because it is the only part that touches entropy, and because
    /// three consecutive fallible draws inside the write would push
    /// [`Runners::register`] past the function-length line for no gain.
    fn mint(&self, now: UnixMillis) -> Result<(Uuid7, Uuid7, Minted)> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy.fill(&mut bytes)?;
        let runner_id = Uuid7::encode(now, bytes)?;
        self.entropy.fill(&mut bytes)?;
        let event_id = Uuid7::encode(now, bytes)?;
        Ok((runner_id, event_id, Minted::draw(&self.entropy)?))
    }

    /// Enrols a runner, storing only the digest of the token it returns.
    ///
    /// The row and its enrolment event land in ONE statement, so an observer
    /// can never see a registered runner with no audit row explaining where it
    /// came from.
    ///
    /// # The initial verdict is reconciled against NO report
    ///
    /// An assignment that demands enforcement therefore opens `degraded` with
    /// "no capability report", and the lease gate refuses work until the host's
    /// first heartbeat proves the cage. That is deliberate: the alternative is
    /// a fail-OPEN window between minting a token and the first beat.
    ///
    /// # Errors
    /// Refuses a `host_id` outside its bounds or a malformed registry
    /// allowlist; reports a datastore that would not answer, and an entropy
    /// source that could not produce a credential.
    pub async fn register(
        &self,
        request: &RegisterRequest<'_>,
        now: UnixMillis,
    ) -> Result<Enrolled> {
        let host_id = HostId::new(&request.host_id)?;
        let stored = assignment(&request.assigned_policy)?;
        let worker_count = stored.worker_count.get();

        // Reconciled against the assignment AS STORED, so the verdict describes
        // the row rather than the request — they differ whenever the clamp bit.
        let assigned = AssignedPolicy {
            worker_count,
            ..request.assigned_policy.clone()
        };
        let verdict = reconcile(Some(&assigned), None);

        // Rendered before the connection is taken: serialisation cannot fail
        // for these shapes, but holding a pooled connection across work that
        // does not need it is how a pool's occupancy stops matching its load.
        let labels_json = render_list(&request.labels);
        let registry_json = render_list(&assigned.registry_allowlist);

        let (runner_id, event_id, token) = self.mint(now)?;
        let mut connection = self.database.acquire().await?;
        sql::runner::RegisterRow {
            runner_id: &runner_id,
            host_id: host_id.as_str(),
            token_digest: token.digest().as_str(),
            sandbox_tier: tier_wire(assigned.sandbox_tier),
            admin_state: sql::ADMIN_STATE_ACTIVE,
            labels_json: &labels_json,
            last_seen_at: sql::LAST_SEEN_NEVER,
            now,
            event_id: &event_id,
            event_type: sql::event_type::RUNNER_REGISTERED,
            network_policy: policy_wire(assigned.network_policy),
            registry_allowlist_json: &registry_json,
            worker_count: worker_count.cast_signed(),
            degraded: verdict.is_degraded(),
            degraded_reason: verdict.reason(),
        }
        .bind()
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT_REGISTER))?;

        let id = runner_id.as_str();
        let host = host_id.as_str();
        let degraded = verdict.is_degraded();
        tracing::debug!(runner_id = id, host_id = host, degraded, "runner enrolled");
        Ok(Enrolled {
            runner_id,
            token,
            worker_count,
            verdict,
        })
    }
}
