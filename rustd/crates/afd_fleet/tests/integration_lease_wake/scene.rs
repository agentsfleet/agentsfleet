//! The scene every readiness-directory test polls: a private index, a store
//! over it with a fresh cursor, and fleets seeded into chosen partitions.

use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_dragonfly::{Partition, ReadyIndex, ReadyPrefix};
use afd_fleet::lease::assign::measured::PollMeasurement;
use afd_fleet::lease::{Acquired, Leases};

use crate::queue;
use crate::requests::ENROLLED_AT;
use crate::seed::{ACTOR, EVENT_TYPE, REQUEST_JSON, seeded_parts};
use crate::support::Fixtures;

/// How many fleets a test may seed looking for one outside a partition set.
const SEED_ATTEMPTS: usize = 64;

/// How long apart two sends write their marks: tokens order by their mint
/// millisecond, so two marks in one millisecond would order at random.
const MARK_GAP: Duration = Duration::from_millis(2);

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

/// A private index, the store polling it, and the fixtures behind both.
pub(super) struct Scene {
    pub(super) fixtures: Fixtures,
    prefix: ReadyPrefix,
    pub(super) index: ReadyIndex,
    pub(super) leases: Leases,
}

/// A scene whose index no other test or run can reach.
pub(super) async fn scene(name: &str) -> Scene {
    let fixtures = Fixtures::create_with_queue().await;
    // Unique per test and per run, so a lane that kept its state cannot hand
    // one run another's directory.
    let run = afd_core::clock::now().as_millis();
    let prefix = ReadyPrefix::private(&format!("wake-{name}-{}-{run}", std::process::id()));
    Scene {
        index: ReadyIndex::under(fixtures.queue().clone(), prefix.clone()),
        leases: fixtures.leases().with_ready_prefix(prefix.clone()),
        prefix,
        fixtures,
    }
}

/// The partition numbered `index`.
pub(super) fn partition(index: u16) -> Partition {
    Partition::new(index).expect("a partition below sixteen")
}

impl Scene {
    /// A fleet whose partition is none of `avoid`, so a test can say which
    /// partition the cursor and the hint each name.
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

    /// Ingress: one event appended, then the fleet marked.
    pub(super) async fn send(&self, fleet: &Fleet) {
        queue::enqueue(
            self.fixtures.queue(),
            &fleet.id,
            &fleet.workspace,
            ACTOR,
            EVENT_TYPE,
            REQUEST_JSON,
            ENROLLED_AT,
        )
        .await;
        self.index
            .mark(&fleet.id)
            .await
            .expect("the private index takes a mark");
        tokio::time::sleep(MARK_GAP).await;
    }

    /// One poll by `runner`, and what it cost.
    pub(super) async fn poll(&self, runner: &Uuid7) -> (Option<Acquired>, PollMeasurement) {
        let (polled, cost) = self
            .leases
            .select_measured(runner, &[], UnixMillis::from_millis(ENROLLED_AT))
            .await;
        (polled.expect("the poll must not fault"), cost)
    }

    /// The partitions the directory names, oldest mark first.
    pub(super) async fn dirty(&self) -> Vec<u16> {
        let entries = self.index.dirty().await.expect("the directory is readable");
        entries
            .iter()
            .map(|entry| entry.partition.index())
            .collect()
    }

    /// Spends every entry, as a lost directory write would leave the index.
    pub(super) async fn lose_directory(&self) {
        for entry in self.index.dirty().await.expect("readable") {
            self.index.spend(&entry).await.expect("the entry deletes");
        }
    }

    /// The directory's physical key.
    pub(super) fn directory_key(&self) -> String {
        self.fixtures
            .queue()
            .physical_key(&self.prefix.directory_key())
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
