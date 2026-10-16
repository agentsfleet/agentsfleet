//! The scene every doorbell test polls: a private index, a store over it with
//! a fresh cursor and a doorbell, a hub ringing that doorbell from the index's
//! own channels, and fleets seeded into chosen partitions.

use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_dragonfly::{Doorbell, FleetStreams, Partition, ReadyIndex, ReadyPrefix, SubscriptionHub};
use afd_fleet::lease::assign::measured::PollMeasurement;
use afd_fleet::lease::{Acquired, Leases};

use crate::queue;
use crate::requests::enrolled_at;
use crate::seed::{ACTOR, EVENT_TYPE, REQUEST_JSON, seeded_parts};
use crate::support::Fixtures;

/// How many fleets a test may seed looking for one outside a partition set.
const SEED_ATTEMPTS: usize = 64;

/// How long a ring may take to reach the doorbell before a test calls it
/// lost; a ring on a live lane arrives in milliseconds.
const HEARD_WITHIN: Duration = Duration::from_secs(3);

/// How often the scene re-sends its probe while the hub's subscriptions are
/// still being confirmed.
const PROBE_EVERY: Duration = Duration::from_millis(50);

/// What the settling probe publishes; the doorbell reads only the channel.
const PROBE: &str = "probe";

/// One seeded fleet: its id, its workspace and its two runners.
pub(super) struct Fleet {
    pub(super) id: String,
    pub(super) workspace: String,
    pub(super) runners: [Uuid7; 2],
}

impl Fleet {
    pub(super) fn partition(&self) -> Partition {
        Partition::of(&self.id)
    }
}

/// A private index, the store polling it, its doorbell, the hub ringing it,
/// and the fixtures behind them all.
pub(super) struct Scene {
    pub(super) fixtures: Fixtures,
    prefix: ReadyPrefix,
    pub(super) index: ReadyIndex,
    pub(super) leases: Leases,
    pub(super) doorbell: Doorbell,
    _hub: SubscriptionHub,
}

/// A scene whose index and channels no other test or run can reach, its
/// sixteen subscriptions confirmed and its doorbell clear.
pub(super) async fn scene(name: &str) -> Scene {
    let fixtures = Fixtures::create_with_queue().await;
    // Unique per test and per run, so a lane that kept its state cannot hand
    // one run another's marks or rings.
    let run = afd_core::clock::now().as_millis();
    let prefix = ReadyPrefix::private(&format!("wake-{name}-{}-{run}", std::process::id()));
    let hub = SubscriptionHub::start(queue::config())
        .await
        .expect("the lane's Dragonfly accepts a subscriber");
    let doorbell = Doorbell::new();
    doorbell.listen(&hub, &prefix);
    settle(&fixtures, &prefix, &doorbell).await;
    Scene {
        index: ReadyIndex::under(fixtures.queue().clone(), prefix.clone()),
        leases: fixtures
            .leases()
            .with_ready_prefix(prefix.clone())
            .with_doorbell(doorbell.clone()),
        doorbell,
        _hub: hub,
        prefix,
        fixtures,
    }
}

/// Probes every channel until each has rung the doorbell once, so a ring a
/// test sends after this is heard, then clears what the probes rang.
async fn settle(fixtures: &Fixtures, prefix: &ReadyPrefix, doorbell: &Doorbell) {
    let streams = FleetStreams::new(fixtures.queue().clone());
    let deadline = tokio::time::Instant::now() + HEARD_WITHIN;
    loop {
        let unheard: Vec<Partition> = Partition::all()
            .filter(|partition| !doorbell.is_rung(*partition))
            .collect();
        if unheard.is_empty() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the doorbell's subscriptions never confirmed: {unheard:?}"
        );
        for partition in unheard {
            let channel = partition.wake_channel_under(prefix);
            streams
                .publish(&channel, PROBE)
                .await
                .expect("the probe publishes");
        }
        tokio::time::sleep(PROBE_EVERY).await;
    }
    while doorbell.take(partition(0)).is_some() {}
    // A probe re-sent while an earlier one was still in flight can land after
    // the clear; wait it out and clear again. Then a quiet window that nothing
    // may ring in, so a probe later still fails here, named, rather than
    // starting a test with a ring it never sent.
    tokio::time::sleep(PROBE_EVERY * 2).await;
    while doorbell.take(partition(0)).is_some() {}
    tokio::time::sleep(PROBE_EVERY * 2).await;
    let stray: Vec<Partition> = Partition::all()
        .filter(|partition| doorbell.is_rung(*partition))
        .collect();
    assert!(stray.is_empty(), "a probe rang after settling: {stray:?}");
}

/// The partition numbered `index`.
pub(super) fn partition(index: u16) -> Partition {
    Partition::new(index).expect("a partition below sixteen")
}

impl Scene {
    /// A fleet whose partition is none of `avoid`, so a test can say which
    /// partition the cursor and the doorbell each name.
    pub(super) async fn fleet(&self, avoid: &[Partition]) -> Fleet {
        let mut found = None;
        for _attempt in 0..SEED_ATTEMPTS {
            let (id, workspace, _tenant, runners) = seeded_parts::<2>(&self.fixtures).await;
            if !avoid.contains(&Partition::of(&id)) {
                found = Some(Fleet {
                    id,
                    workspace,
                    runners,
                });
                break;
            }
        }
        found.expect("a seeded fleet lands outside a few partitions within the attempts")
    }

    /// Ingress: one event appended, then the fleet marked and its ring heard.
    pub(super) async fn send(&self, fleet: &Fleet) {
        self.enqueue(fleet).await;
        self.mark(fleet).await;
    }

    /// One event appended and the fleet marked without a ring, as a mark whose
    /// ring was lost leaves it.
    pub(super) async fn send_quietly(&self, fleet: &Fleet) {
        self.enqueue(fleet).await;
        self.index
            .mark_quiet(&fleet.id)
            .await
            .expect("the private index takes a mark");
    }

    /// One event appended to the fleet's stream.
    ///
    /// The shared enqueue also marks the production index, which this scene
    /// never polls. That mark is cleared here: these scenes fill partitions to
    /// their ceiling on purpose, and left in the production index the fill
    /// would crowd every other suite's fleet that hashes there.
    async fn enqueue(&self, fleet: &Fleet) {
        queue::enqueue(
            self.fixtures.queue(),
            &fleet.id,
            &fleet.workspace,
            ACTOR,
            EVENT_TYPE,
            REQUEST_JSON,
            enrolled_at(),
        )
        .await;
        queue::clear_ready(self.fixtures.queue(), &fleet.id).await;
    }

    /// The fleet marked, and its ring heard by this scene's doorbell.
    pub(super) async fn mark(&self, fleet: &Fleet) {
        self.index
            .mark(&fleet.id)
            .await
            .expect("the private index takes a mark");
        let deadline = tokio::time::Instant::now() + HEARD_WITHIN;
        while !self.rung(fleet.partition()) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the mark's ring never reached the doorbell"
            );
            tokio::task::yield_now().await;
        }
    }

    /// Whether the doorbell holds `partition` rung.
    pub(super) fn rung(&self, partition: Partition) -> bool {
        self.doorbell.is_rung(partition)
    }

    /// One poll by `runner`, and what it cost.
    pub(super) async fn poll(&self, runner: &Uuid7) -> (Option<Acquired>, PollMeasurement) {
        let (polled, cost) = self
            .leases
            .select_measured(runner, &[], UnixMillis::from_millis(enrolled_at()))
            .await;
        (polled.expect("the poll must not fault"), cost)
    }

    /// `partition`'s physical key in this scene's index.
    pub(super) fn partition_key(&self, partition: Partition) -> String {
        self.fixtures
            .queue()
            .physical_key(&partition.key_under(&self.prefix))
    }

    /// Sends `verb` at `key`: a test puts a wrong type there to break it,
    /// then deletes it.
    pub(super) async fn raw(&self, key: &str, verb: &'static str, value: Option<&str>) {
        let mut cmd = redis::cmd(verb);
        cmd.arg(key);
        if let Some(value) = value {
            cmd.arg(value);
        }
        let _: redis::Value = self
            .fixtures
            .queue()
            .command(verb, key, &cmd)
            .await
            .expect(verb);
    }
}
