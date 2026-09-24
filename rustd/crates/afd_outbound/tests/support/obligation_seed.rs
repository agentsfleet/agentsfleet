//! The three parent rows `core.fleet_obligations` hangs from.
//!
//! The obligation table references `core.fleets` and `core.workspaces`, which
//! reference `core.tenants`, so an obligation cannot be written until all three
//! exist. Every crate that needs them seeds its own — `afd_api`, `afd_approval`
//! and `afd_fleet` each carry a copy — because there is no shared fixture crate
//! and a test that borrowed another crate's would depend on that crate's test
//! tree. This is the smallest seed that satisfies the constraints and nothing
//! more: no lease, no admission, no stream.
//!
//! # Why the ids are fixed and the names carry them
//!
//! `uq_workspaces_tenant_id_name` is unique per tenant and every lane here
//! shares one tenant, so a constant NAME would collide where the `ON CONFLICT
//! (id)` arm cannot see it. The ids are v7 spellings because
//! `ck_fleet_obligations_id_uuidv7` checks the version nibble, and a v4 id
//! would be refused by the CHECK rather than by anything a reader can see.

#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_db::Db;
use afd_dragonfly::{
    Dedicated, Dragonfly, OUTBOUND_CONSUMER_GROUP, OUTBOUND_STREAM_KEY, OutboundReader,
};

/// The instant every fixture row is stamped with.
///
/// Fixed rather than `now()`: the scans this suite drives take their cutoff as
/// a parameter, so a test picks a cutoff relative to THIS and never waits on a
/// wall clock. `producer::MIN_AGE` is thirty seconds and `LOST_AFTER` is five
/// minutes; a suite that honoured them against the real clock would sleep for
/// both.
pub(crate) const SEEDED_AT: i64 = 1_760_000_000_000;

/// The tenant every fixture workspace hangs from.
pub(crate) const TENANT: &str = "0195b4ba-8d3a-7a11-8abc-000000000001";
/// The workspace every fixture fleet hangs from.
pub(crate) const WORKSPACE: &str = "0195b4ba-8d3a-7a11-8abc-000000000002";
/// The fleet every fixture obligation is owed by.
pub(crate) const FLEET: &str = "0195b4ba-8d3a-7a11-8abc-000000000003";

/// Seeds the tenant, workspace and fleet, idempotently.
///
/// `ON CONFLICT (id) DO NOTHING` throughout, because the suite shares one
/// schema (`TestDatabase::shared`) and every test calls this.
pub(crate) async fn seed_parents(database: &Db) {
    let mut connection = database.acquire().await.expect("the ledger answers");

    sqlx::query(
        "INSERT INTO core.tenants (id, name, created_at, updated_at)
         VALUES ($1::uuid, 'outbound-fixture', $2, $2)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(TENANT)
    .bind(SEEDED_AT)
    .execute(&mut *connection)
    .await
    .expect("seeding a tenant");

    sqlx::query(
        "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
         VALUES ($1::uuid, $2::uuid, $1::text, 'fixture', $3)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(WORKSPACE)
    .bind(TENANT)
    .bind(SEEDED_AT)
    .execute(&mut *connection)
    .await
    .expect("seeding a workspace");

    sqlx::query(
        "INSERT INTO core.fleets
           (id, workspace_id, tenant_id, name, source_markdown, config_json,
            status, created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3::uuid, 'outbound-fixture', '# fixture',
                 '{}'::jsonb, 'active', $4, $4)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(FLEET)
    .bind(WORKSPACE)
    .bind(TENANT)
    .bind(SEEDED_AT)
    .execute(&mut *connection)
    .await
    .expect("seeding a fleet");
}

/// Removes every obligation this suite owes, so each test starts owing nothing.
///
/// Scoped to the fixture fleet rather than the table, because the shared schema
/// carries other suites' rows and a bare `DELETE` would take theirs too.
pub(crate) async fn clear_obligations(database: &Db) {
    let mut connection = database.acquire().await.expect("the ledger answers");
    sqlx::query("DELETE FROM core.fleet_obligations WHERE fleet_id = $1::uuid")
        .bind(FLEET)
        .execute(&mut *connection)
        .await
        .expect("clearing this fixture's obligations");
}

/// The thread every fixture answer is addressed to, as a Slack producer
/// records it.
pub(crate) const DESTINATION: &str =
    r#"{"team_id":"T024BE7LD","channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;

/// One owed row, written straight into the ledger.
pub(crate) struct OwedRow<'a> {
    /// Which [`obligation_id`] the row takes.
    pub(crate) nth: u8,
    pub(crate) event: &'a str,
    /// The connector id the row names, which need not be one any connector
    /// answers to.
    pub(crate) provider: &'a str,
    /// `None` for a row naming nowhere.
    pub(crate) destination: Option<&'a str>,
    pub(crate) receipt: Option<&'a str>,
    pub(crate) answer: &'a str,
}

/// Writes `row` owed by the fixture fleet, bypassing the ledger's own verbs.
///
/// For the rows those verbs refuse to write and a deployment can still hold:
/// one naming a connector the catalogue no longer answers to, or one the report
/// path wrote before it read a destination.
pub(crate) async fn seed_owed_row(database: &Db, row: OwedRow<'_>) {
    let mut connection = database.acquire().await.expect("the ledger answers");
    sqlx::query(
        "INSERT INTO core.fleet_obligations
           (id, fleet_id, workspace_id, provider, destination, event_id, answer,
            receipt, attempt_count, created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, 0, $9, $9)",
    )
    .bind(obligation_id(row.nth))
    .bind(FLEET)
    .bind(WORKSPACE)
    .bind(row.provider)
    .bind(row.destination)
    .bind(row.event)
    .bind(row.answer)
    .bind(row.receipt)
    .bind(SEEDED_AT)
    .execute(&mut *connection)
    .await
    .expect("seeding an owed row");
}

/// The fixture fleet's row for `event`, as its abandonment reads:
/// `(abandoned_at, abandon_reason)`, both `None` while it is still owed.
pub(crate) async fn abandonment(database: &Db, event: &str) -> (Option<i64>, Option<String>) {
    let mut connection = database.acquire().await.expect("the ledger answers");
    sqlx::query_as(
        "SELECT abandoned_at, abandon_reason FROM core.fleet_obligations
          WHERE fleet_id = $1::uuid AND event_id = $2::text",
    )
    .bind(FLEET)
    .bind(event)
    .fetch_one(&mut *connection)
    .await
    .expect("reading the obligation")
}

/// A v7-shaped obligation id, distinct per caller.
///
/// `ck_fleet_obligations_id_uuidv7` reads the version nibble, so the `7` in the
/// third group is load-bearing and not decoration.
pub(crate) fn obligation_id(nth: u8) -> String {
    format!("{OBLIGATION_ID_STEM}{nth:02}")
}

/// Everything before the two digits [`obligation_id`] varies.
///
/// Its own constant beside the three ids above, so the v7 version nibble is
/// declared once and in the same place as theirs — the CHECK that reads it
/// rejects a v4 spelling, and a stem inlined in a `format!` is where that would
/// drift unseen.
const OBLIGATION_ID_STEM: &str = "0195b4ba-8d3a-7a11-8abc-1000000000";

/// Commands this module issues directly, named once each (RULE UFS).
const CMD_XGROUP: &str = "XGROUP";
/// See [`CMD_XGROUP`].
const CMD_DESTROY: &str = "DESTROY";
/// See [`CMD_XGROUP`].
const CMD_DEL: &str = "DEL";
/// See [`CMD_XGROUP`].
const CMD_XLEN: &str = "XLEN";
/// See [`CMD_XGROUP`].
const CMD_XRANGE: &str = "XRANGE";

/// `XRANGE`'s whole-stream bounds.
const RANGE_START: &str = "-";
const RANGE_END: &str = "+";

/// The entry field naming the logical event an answer belongs to; written by
/// `OutboundQueue::enqueue`.
const FIELD_EVENT_ID: &str = "event_id";
/// The job field the destination is appended under.
const FIELD_DESTINATION: &str = "destination";

/// Destroys the consumer group, leaving the stream and its entries in place.
///
/// The "lost group" half of Dimension 7.6, and the reason it is its own test:
/// the entries survive and become unreachable, which is a different shape from
/// losing the stream even though the ledger cannot tell them apart.
pub(crate) async fn forget_group(redis: &Dragonfly) {
    let mut cmd = redis::cmd(CMD_XGROUP);
    cmd.arg(CMD_DESTROY)
        .arg(OUTBOUND_STREAM_KEY)
        .arg(OUTBOUND_CONSUMER_GROUP);
    let _destroyed: i64 = redis
        .command(CMD_XGROUP, OUTBOUND_STREAM_KEY, &cmd)
        .await
        .expect("the group this fixture created must be destroyable");
}

/// Removes the stream entirely — entries, groups and pending lists together.
pub(crate) async fn forget_stream(redis: &Dragonfly) {
    let mut cmd = redis::cmd(CMD_DEL);
    cmd.arg(OUTBOUND_STREAM_KEY);
    let _removed: i64 = redis
        .command(CMD_DEL, OUTBOUND_STREAM_KEY, &cmd)
        .await
        .expect("the outbound stream must be removable");
}

/// How many entries the stream holds, whoever is holding them.
pub(crate) async fn entries_on(redis: &Dragonfly) -> u64 {
    let mut cmd = redis::cmd(CMD_XLEN);
    cmd.arg(OUTBOUND_STREAM_KEY);
    redis
        .command(CMD_XLEN, OUTBOUND_STREAM_KEY, &cmd)
        .await
        // NOT `unwrap_or(0)`: three tests assert this is ZERO to prove nothing
        // was queued, or that a deleted stream is gone. A swallowed failure
        // returns the very value those assertions are looking for, so an
        // unreachable server would pass all three while proving nothing.
        .expect("XLEN answers on a live stream, and a missing key reads as 0")
}

/// How many entries on the stream carry `event`.
///
/// # Why a count of the whole stream will not do
///
/// [`entries_on`] answers for the stream, and the stream is one key for the
/// deployment. `Producer::run` scans `core.fleet_obligations` deployment-WIDE
/// and appends every row it finds owed — which is correct, and which means a
/// test asserting "appended exactly once" against `XLEN` is really asserting
/// that no other fleet in the shared database owed anything. On a lane that
/// has run hundreds of tests that is never true, and the failure reads as a
/// duplicate append the producer never made.
///
/// So the question is asked about this test's own answer. Scanned with
/// `XRANGE` rather than read through the group, because a read would claim the
/// entries and change the pending list this suite asserts on.
pub(crate) async fn entries_naming(redis: &Dragonfly, event: &str) -> u64 {
    fields_naming(redis, event)
        .await
        .len()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// The destination every entry carrying `event` was appended with.
///
/// Scoped to this test's own answer for the reason [`entries_naming`] gives.
pub(crate) async fn destinations_naming(redis: &Dragonfly, event: &str) -> Vec<String> {
    fields_naming(redis, event)
        .await
        .into_iter()
        .filter_map(|fields| {
            fields
                .into_iter()
                .find_map(|(key, value)| (key == FIELD_DESTINATION).then_some(value))
        })
        .collect()
}

/// Each entry on the stream that carries `event`, as its field pairs.
async fn fields_naming(redis: &Dragonfly, event: &str) -> Vec<Vec<(String, String)>> {
    let mut cmd = redis::cmd(CMD_XRANGE);
    cmd.arg(OUTBOUND_STREAM_KEY).arg(RANGE_START).arg(RANGE_END);
    let entries: Vec<(String, Vec<String>)> = redis
        .command(CMD_XRANGE, OUTBOUND_STREAM_KEY, &cmd)
        .await
        .expect("XRANGE answers on a live stream, and a missing key reads as empty");
    entries
        .into_iter()
        .map(|(_id, fields)| {
            fields
                .as_chunks::<2>()
                .0
                .iter()
                .map(|[key, value]| (key.clone(), value.clone()))
                .collect::<Vec<_>>()
        })
        .filter(|fields| {
            fields
                .iter()
                .any(|(key, value)| key == FIELD_EVENT_ID && value == event)
        })
        .collect()
}

/// A reader under a consumer name of the caller's choosing.
///
/// [`afd_dragonfly::outbound_consumer`] is host-derived and constant for a
/// process, which is exactly why a replacement under a DIFFERENT hostname
/// cannot be staged with it: one test process has one hostname. Naming the
/// consumer explicitly is how two hosts are staged inside one process, and the
/// name is the only thing that differs from what production builds.
pub(crate) async fn reader_named(
    config: &afd_dragonfly::config::DragonflyConfig,
    host: &str,
) -> OutboundReader {
    let connection = Dedicated::connect(config, afd_outbound::LONGEST_PARK)
        .await
        .expect("a dedicated connection must be openable");
    OutboundReader::new(connection, host.to_owned())
}
