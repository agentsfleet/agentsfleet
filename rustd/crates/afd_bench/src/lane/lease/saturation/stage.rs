//! The population the saturation shape polls: chat drafts across tenants,
//! ready fleets beside them, and hosts that hold several leases at once.
//!
//! # Seeded in the order the keys and the paths require
//!
//! 1. One tenant, workspace and owner per chat tenant, each funded deep, so
//!    no wallet gate answers instead of the path under measurement.
//! 2. The platform default every draft names, sealed into the first chat
//!    tenant's workspace: `Drafts::create` refuses a model it cannot resolve.
//! 3. The drafts, through `Drafts::create` itself, each with one queued
//!    message — the state a person sending a first chat message leaves.
//! 4. One ready fleet per `BENCH_FLEETS`, its event admitted the way ingress
//!    admits it, so a poll alternates between both kinds as it does in
//!    production.
//! 5. The hosts, each declaring [`WORKERS`] workers.

use std::sync::Arc;

use afd_authoring::Access;
use afd_authoring::Drafts;
use afd_authoring::types::{CreateDraft, Source};
use afd_core::clock;
use afd_core::id::Uuid7;
use afd_credential::provider::PLATFORM_MODEL_REF;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::test_util::{UserSeed, WalletSeed};
use afd_wire::event::EventType;

use crate::datastores::Datastores;
use crate::error::{Result, seed_refused};
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::lane::lease::Parameters;
use crate::lane::lease::drain::platform;
use crate::lane::lease::seed::{self, ROWS_PER_FLEET, ROWS_PER_RUNNER, SEEDED_AT, SeededFleet};

/// Where the saturation shape's identifiers start, clear of the drain's and
/// the contended window's.
pub(super) const INDEX_BASE: u64 = 0x2000_0000;

/// Tenants whose drafts compete for one poll.
///
/// Several, because the claim this shape exists to measure walks candidates
/// across tenants, and one tenant would never show one stalling another.
pub(super) const TENANTS: u64 = 5;

/// Queued drafts per chat tenant.
const DRAFTS_PER_TENANT: u64 = 8;

/// Concurrent workers each saturation host declares.
///
/// Above one so a host holding an unreported chat lease keeps polling, and
/// the window stays contended rather than going busy after one lease each.
pub(super) const WORKERS: u32 = 4;

/// A balance no saturation run can spend.
const DEEP_POOL_NANOS: i64 = 1_000_000_000_000;

/// Identifier kind for a chat tenant's owner, apart from the seed's and the
/// platform row's.
const KIND_USER: u32 = 5;

/// The text every seeded chat message carries.
///
/// Generated, never a tenant's words, for the reason the seed's request body
/// gives (RULE PRI).
const MESSAGE: &str = "bench";

/// Suffix of the shape's placement tag.
const TAG_SUFFIX: &str = "saturation-tag";

/// Suffix of a saturation host's id.
const HOST_SUFFIX: &str = "saturation-host";

/// Rows a chat tenant inserts that the prefix sweep counts: its tenant,
/// workspace and fleet. The owner, wallet and drafts cascade with them.
const ROWS_PER_CHAT_TENANT: u64 = ROWS_PER_FLEET;

/// What the shape seeded, and what it must release itself.
#[derive(Debug)]
pub(super) struct Staged {
    /// The chat tenants, their workspaces and their (idle) fleets.
    pub(super) tenants: Vec<SeededFleet>,
    /// The ready fleets, one event each.
    pub(super) fleets: Vec<SeededFleet>,
    /// Queued chat messages, one per draft.
    pub(super) messages: u64,
    /// The hosts that poll.
    pub(super) runners: Vec<Uuid7>,
}

impl Staged {
    /// Leases the window can issue: one per queued message and one per event,
    /// but no more than the hosts have workers, because nothing is reported
    /// and a full host takes no more. Whichever runs out first ends the window,
    /// so a rate is never leases divided by a deadline spent idle.
    pub(super) fn population(&self) -> u64 {
        let work = self.messages + u64::try_from(self.fleets.len()).unwrap_or(u64::MAX);
        let hosts = u64::try_from(self.runners.len()).unwrap_or(u64::MAX);
        work.min(hosts.saturating_mul(u64::from(WORKERS)))
    }
}

/// Seed the saturation population and the platform rows its drafts resolve,
/// sealing the platform credential under `kek`.
///
/// # Errors
///
/// Whatever Postgres, Dragonfly or the setup chat path refused, and a
/// platform default another run already holds.
pub(super) async fn stage(
    stores: &Datastores,
    drafts: &Drafts,
    kek: &Arc<Kek>,
    prefix: &RunPrefix,
    parameters: Parameters,
    ledger: &mut FixtureLedger,
) -> Result<Staged> {
    let tag = prefix.name(TAG_SUFFIX);
    let mut staged = Staged {
        tenants: Vec::new(),
        fleets: Vec::new(),
        messages: 0,
        runners: Vec::new(),
    };
    for index in 0..TENANTS {
        let tenant = seed::identities(INDEX_BASE + index);
        seed::rows(&stores.database, prefix, &tenant, &tag, SEEDED_AT).await?;
        ledger.created(ROWS_PER_CHAT_TENANT);
        owner(stores, &tenant, index).await?;
        staged.tenants.push(tenant);
    }
    platform::stage(stores, kek, INDEX_BASE).await?;
    for (index, tenant) in (0..TENANTS).zip(&staged.tenants) {
        staged.messages += queue(drafts, tenant, index).await?;
    }
    for index in 0..parameters.fleets {
        let fleet = seed::identities(INDEX_BASE + TENANTS + index);
        seed::rows(&stores.database, prefix, &fleet, &tag, SEEDED_AT).await?;
        fund(stores, &fleet).await?;
        seed::enqueue(&stores.database, &stores.queue, &fleet, EventType::Chat).await?;
        ledger.created(ROWS_PER_FLEET);
        staged.fleets.push(fleet);
    }
    for index in 0..parameters.runners {
        let host = prefix.name(&format!("{HOST_SUFFIX}-{index}"));
        let runner = seed::runner_with(&stores.database, &host, &tag, WORKERS, SEEDED_AT).await?;
        staged.runners.push(runner);
        ledger.created(ROWS_PER_RUNNER);
    }
    Ok(staged)
}

/// The chat tenant's owner, whose subject the drafts are created under, and
/// its wallet.
async fn owner(stores: &Datastores, tenant: &SeededFleet, index: u64) -> Result<()> {
    let user = subject(index);
    let mut connection = stores.database.acquire().await?;
    UserSeed::new(&user, &tenant.tenant, &user, &user)
        .insert(&mut *connection)
        .await
        .map_err(seed_refused)?;
    drop(connection);
    fund(stores, tenant).await
}

/// Give a tenant a deep wallet.
async fn fund(stores: &Datastores, tenant: &SeededFleet) -> Result<()> {
    let mut connection = stores.database.acquire().await?;
    WalletSeed::new(&tenant.tenant, DEEP_POOL_NANOS)
        .at(SEEDED_AT)
        .insert(&mut *connection)
        .await
        .map_err(seed_refused)
}

/// Queue [`DRAFTS_PER_TENANT`] first messages for one chat tenant, through
/// the route's own create verb, answering how many were queued.
async fn queue(drafts: &Drafts, tenant: &SeededFleet, index: u64) -> Result<u64> {
    let workspace = Uuid7::parse(&tenant.workspace)?;
    let user = subject(index);
    let access = Access {
        workspace: &workspace,
        subject: &user,
    };
    for _ in 0..DRAFTS_PER_TENANT {
        let now = clock::now();
        let request = CreateDraft {
            source: Source::Blank,
            authoring_model_ref: PLATFORM_MODEL_REF.to_owned(),
            message: Some(MESSAGE.to_owned()),
            operation_id: Entropy::new().uuid7(now)?,
        };
        drafts.create(access, request, now).await?;
    }
    Ok(DRAFTS_PER_TENANT)
}

/// The owner's id, which is also the subject and address it signs in with.
fn subject(index: u64) -> String {
    seed::identifier(KIND_USER, INDEX_BASE + index)
}
