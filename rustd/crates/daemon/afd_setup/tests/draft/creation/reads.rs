//! A read of a draft or of a saved fleet holds no row lock, so a write or an
//! intake it overlaps never queues behind it.
//!
//! The lane's owner holds one table the read reaches only after it read its
//! own rows, and that the raced work never reads, so the read parks
//! mid-transaction while the work runs. Each case owns its database: the
//! table lock would stall every other proof.

use super::super::{fixture::now, mint};
use super::fixture::Case;
use af_crypto::entropy::Entropy;
use af_wire::turn::TurnKind;
use afd_setup::types::{MessageQuery, SendMessage, Start};
use afd_turns::{IncomingTurn, Key, Producer, Reply, Turns};
use sqlx::{Connection as _, PgConnection};

/// The table a message page and a stream frame read usage from, after the
/// draft, and that a send never reads.
const LOCK_USAGE: &str = "LOCK TABLE billing.usage_lines IN ACCESS EXCLUSIVE MODE";

/// The table a review's credential inspection reads, after the fleet row, and
/// that an intake never reads.
const LOCK_SECRETS: &str = "LOCK TABLE vault.secrets IN ACCESS EXCLUSIVE MODE";

/// The lane owner's open transaction holding `lock`, and its backend.
async fn hold(c: &Case, lock: &'static str) -> (PgConnection, i32) {
    let mut owner = PgConnection::connect(&c.base.owner_url())
        .await
        .expect("the lane owner connects");
    sqlx::query("BEGIN")
        .execute(&mut owner)
        .await
        .expect("the holder opens");
    let pid = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut owner)
        .await
        .expect("the holder's backend");
    sqlx::query(lock)
        .execute(&mut owner)
        .await
        .expect("the owner takes the table");
    (owner, pid)
}

async fn release(mut owner: PgConnection) {
    sqlx::query("ROLLBACK")
        .execute(&mut owner)
        .await
        .expect("the owner releases the table");
    owner.close().await.expect("the owner disconnects");
}

// Value: protects=a draft's message page and stream frame, parked after their
// draft read, hold no lock a send waits on, and each reads one snapshot, so
// neither shows the message the send committed while it ran; fails_when=either
// read takes the draft row lock (`lock.sql`) or the person's again, and the
// send waits out the login's lock bound, or a read drops its snapshot and
// shows the second message; seam=Drafts::messages, Drafts::initial_view and
// Drafts::send over a private live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_draft_read_blocks_no_writer() {
    let c = Case::isolated().await;
    let f = &c.base;
    let draft = f
        .drafts
        .create(f.access(), f.request(mint(), Some("Watch checkout")), now())
        .await
        .expect("a draft with its first message")
        .draft
        .id;
    let (owner, pid) = hold(&c, LOCK_USAGE).await;
    let page = MessageQuery {
        limit: 25,
        starting_after: None,
    };
    let second = SendMessage {
        message: "Now watch billing".to_owned(),
        save: false,
        operation_id: mint(),
    };

    let (read, view, sent) = tokio::join!(
        f.drafts.messages(f.access(), &draft, page),
        f.drafts.initial_view(f.access(), &draft),
        async {
            f.waits_on(pid, 2).await;
            let sent = f.drafts.send(f.access(), &draft, second, now()).await;
            release(owner).await;
            sent
        }
    );

    let sent = sent.expect("the send commits while both reads are in flight");
    assert_eq!(sent.sequence, "2");
    let read = read.expect("the page answers once the table is free");
    let shown: Vec<_> = read
        .items
        .iter()
        .map(|item| item.sequence.as_str())
        .collect();
    assert_eq!(shown, ["1"], "the page reads the draft as it began");
    let view = view.expect("the frame answers once the table is free");
    let noticed: Vec<_> = view
        .notices
        .iter()
        .filter_map(|notice| notice.sequence.as_deref())
        .collect();
    assert_eq!(noticed, ["1"], "the frame reads the draft as it began");
    c.cleanup().await;
}

// Value: protects=a saved fleet's read, parked in its review after the fleet
// row, holds no lock the fleet's intake waits on; fails_when=the read takes
// the fleet row `FOR UPDATE` (`creation/sql/lock.sql`) again, and the intake's
// `FOR NO KEY UPDATE` waits out the login's lock bound; seam=Creations::read
// and Turns::admit over a private live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_setup_read_blocks_no_intake() {
    let c = Case::isolated().await;
    c.fund().await;
    let (key, request) = c.request("intake-watch", Start::None).await;
    let fleet = c.save(&key, request).await.id;
    let turns = Turns::new(c.base.db.clone(), c.bus.clone(), Entropy::new());
    let (owner, pid) = hold(&c, LOCK_SECRETS).await;
    let producer_key = format!("{fleet}:{}", mint());
    let incoming = IncomingTurn {
        producer: Producer::Steer,
        key: Key::Repeated(&producer_key),
        fleet: &fleet,
        workspace: &c.base.workspace,
        actor: c.base.access().subject,
        event_type: TurnKind::Chat,
        request_json: r#"{"message":"Check the checkout"}"#,
        reply: Reply::None,
    };

    let (read, admitted) = tokio::join!(c.creations.read(c.base.access(), &fleet), async {
        c.base.waits_on(pid, 1).await;
        let admitted = turns.admit(incoming).await;
        release(owner).await;
        admitted
    });

    let admitted = admitted.expect("the intake commits while the read is in flight");
    let read = read.expect("the read answers once the table is free");
    assert_eq!(read.id, fleet);
    assert_eq!(
        c.turns(&fleet).await,
        [admitted.stored.id],
        "the fleet holds the admitted turn"
    );
    c.cleanup().await;
}
