#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]

use std::sync::Arc;

use afd_connector::Provider;
use afd_core::env::MapEnv;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::{Db, DbRole, PoolConfig};
use afd_dragonfly::{Dragonfly, DragonflyConfig, DragonflyRole};
use afd_ingress::slack::{ChannelId, MentionAdmission, NoticeOwed, Resident};
use afd_ingress::{Binding, Delivery, Ingress, Surface};
use afd_vault::Vault;
use jiff::Timestamp;

use super::WebhookIngress;

/// A Postgres nobody is listening on. Port 1 is reserved and unbound, so an
/// acquire fails on connection REFUSAL rather than waiting out a timeout.
const NOWHERE: &str = "postgres://runner:secret@127.0.0.1:1/agentsfleet";

/// The same, for the queue.
const NOWHERE_QUEUE: &str = "redis://127.0.0.1:1";

/// The knob bounding how long an acquire may spend before it reports.
const ACQUIRE_TIMEOUT_KNOB: &str = "DATABASE_ACQUIRE_TIMEOUT_MS";

/// Long enough that a refused connect is classified as UNAVAILABLE rather
/// than as pool capacity, short enough that seven of them cost nothing.
const ACQUIRE_TIMEOUT_MS: &str = "50";

/// The empty JSON object the fixtures send where a body or address is unused.
const EMPTY_OBJECT: &str = "{}";

/// The production ingress, over three handles that answer nothing.
fn refusing() -> Ingress {
    let environment = MapEnv::from_pairs([
        (DbRole::Api.url_knob(), NOWHERE),
        (ACQUIRE_TIMEOUT_KNOB, ACQUIRE_TIMEOUT_MS),
    ]);
    let pool = PoolConfig::resolve(&environment, DbRole::Api)
        .expect("the fixture connection string is well formed");
    let database = Db::unreachable(&pool);
    let queue = Dragonfly::unreachable(
        &DragonflyConfig::from_url(DragonflyRole::Default, NOWHERE_QUEUE.to_owned())
            .with_request_timeout(std::time::Duration::from_millis(250)),
    )
    .expect("a lazy manager opens no socket, so it cannot fail to open one");
    let vault = Vault::new(
        database.clone(),
        Arc::new(Kek::from_bytes([7u8; 32])),
        Entropy::new(),
    );
    let admissions = afd_admission::Admissions::for_tests(database.clone(), queue);
    Ingress::new(database, vault, admissions)
}

/// Whether a reader refused.
///
/// A function rather than `assert!(… .is_err())` at each call site: the
/// manifest denies `assertions_on_result_states`, and its suggested
/// `unwrap_err` is denied too. Asking the question once keeps both
/// satisfied and reads better than either.
const fn refused<T, E>(answer: &Result<T, E>) -> bool {
    answer.is_err()
}

/// The fleet every reader is asked about.
fn fleet() -> Uuid7 {
    Uuid7::parse("019329c5-0000-7000-8000-0000000000c1").expect("the fixture fleet is canonical")
}

/// The workspace that fleet lives in.
fn workspace() -> Uuid7 {
    Uuid7::parse("019329c5-0000-7000-8000-0000000000c2")
        .expect("the fixture workspace is canonical")
}

/// A binding to hand the readers that take one.
fn binding() -> Binding {
    Binding::stored(
        fleet(),
        workspace(),
        "active",
        &serde_json::json!({
            "name": "adapter",
            "x-agentsfleet": {
                "triggers": [{"type": "webhook", "source": "github"}],
                "tools": ["bash"],
                "budget": {"daily_nanos": afd_core::money::NANOS_PER_USD.to_string()},
            },
        })
        .to_string(),
        None,
    )
    .expect("the fixture document parses")
    .expect("the fixture document declares a webhook trigger")
}

/// Every method on the seam reaches the store it names.
///
/// Seven one-line delegations, and the reason they are worth a test is that
/// the compiler cannot tell them apart: `svix_secret` forwarding to
/// `signing_secret` type-checks perfectly and silently verifies a Svix
/// delivery against the HMAC family's field — a security boundary crossed
/// by a copied line. Reaching a refusing store proves each one arrives
/// somewhere rather than at its neighbour.
///
/// The router suites cannot cover this: they substitute a stub FOR the
/// trait, so the production impl below is reached by the daemon and by
/// nothing else.
#[tokio::test]
async fn every_reader_on_the_seam_reaches_a_store() {
    let ingress = refusing();
    let fleet = fleet();
    let held = binding();

    assert!(refused(
        &WebhookIngress::binding(&ingress, &fleet, None).await
    ));
    assert!(refused(
        &WebhookIngress::signing_secret(&ingress, &held).await
    ));
    // The one reader that answers WITHOUT a store, and the asymmetry is the
    // point: a trigger declaring no Svix ref has no Svix secret, which is a
    // configuration fact rather than a failure. Reaching the vault to
    // discover it would make an outage and an unconfigured fleet the same
    // answer, and only one of them is worth waking somebody for.
    assert!(
        matches!(WebhookIngress::svix_secret(&ingress, &held).await, Ok(None)),
        "a trigger with no signature ref short-circuits before the vault"
    );
    assert!(refused(
        &WebhookIngress::platform_secret(&ingress, &fleet, "github-app").await
    ));
    assert!(refused(
        &WebhookIngress::installation_workspace(&ingress, "github", "1").await
    ));
    assert!(refused(
        &WebhookIngress::subscribers(&ingress, &fleet, "github", "o/r", "push").await
    ));
    assert!(refused(
        &WebhookIngress::deliver(
            &ingress,
            Surface::Fleet,
            &held,
            &Delivery {
                event_id: "adapter",
                actor: "webhook:github",
                request_json: EMPTY_OBJECT,
            },
        )
        .await
    ));
}

/// One Slack mention of `fleet`, as the admission reader takes it.
fn mention<'a>(fleet: &'a Uuid7, workspace: &'a Uuid7) -> MentionAdmission<'a> {
    MentionAdmission {
        fleet,
        workspace,
        team_id: "TSEAM",
        event_id: "EvSeam",
        user: "U0SEAM",
        request_json: EMPTY_OBJECT,
        connector: "slack",
        address: EMPTY_OBJECT,
    }
}

/// The notice that mention owes, posted by `fleet` as the resident.
fn notice<'a>(fleet: &'a Uuid7, workspace: &'a Uuid7) -> NoticeOwed<'a> {
    NoticeOwed {
        resident: fleet,
        workspace,
        provider: Provider::Slack,
        key: "TSEAM:EvSeam:notice",
        address: EMPTY_OBJECT,
        text: "notice",
    }
}

/// The chat-mention half of the seam reaches its store too.
///
/// Six more one-line delegations with the same exposure: the workspace
/// argument threads through `resident` and `bind_resident` by position, and a
/// swapped pair still type-checks.
#[tokio::test]
async fn every_mention_reader_on_the_seam_reaches_a_store() {
    let ingress = refusing();
    let (fleet, workspace) = (fleet(), workspace());
    let channel: ChannelId = "C0123456789".parse().expect("the channel is well formed");
    let resident = Resident::for_channel("TSEAM", &channel).expect("the resident is named");
    let now = Timestamp::UNIX_EPOCH;

    assert!(refused(
        &WebhookIngress::mention_subscribers(&ingress, &workspace, "slack", &channel).await
    ));
    assert!(refused(
        &WebhookIngress::admit_mention(&ingress, mention(&fleet, &workspace)).await
    ));
    assert!(refused(
        &WebhookIngress::resident(&ingress, &workspace, "slack", "TSEAM", &channel).await
    ));
    assert!(refused(
        &WebhookIngress::bind_resident(
            &ingress, &workspace, "slack", "TSEAM", &channel, &fleet, now
        )
        .await
    ));
    assert!(refused(
        &WebhookIngress::resident_named(&ingress, &workspace, &resident).await
    ));
    assert!(refused(
        &WebhookIngress::owe_notice(&ingress, notice(&fleet, &workspace), now).await
    ));
}
