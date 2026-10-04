//! Flips under contention: a second flip refused while one copies, a route
//! replaced mid-copy that the flip leaves in place, a flip its caller drops
//! before it switches, a write that reaches the store being left first, two flips of one
//! workspace racing on two threads, and route swaps for many workspaces at
//! once.
//!
//! The last two race real threads, so which line a loser takes is the
//! scheduler's choice; what each asserts holds on every interleaving.
#![expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]

use std::sync::{Arc, Barrier, mpsc};
use std::thread;
use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_core::error_code::{INTERNAL_OPERATION_FAILED, MEM_UNAVAILABLE};
use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_db::test_util::unreachable_db;
use afd_wire::memory::Visibility;
use tracing::Level;

use super::{FLEETS, Hold, PUSHED_AT, Rigged, SOURCE, TARGET, WORKSPACE, delta, id, rows, seeded};
use crate::error::detail::MOVING;
use crate::flip::{EVENT_COMPLETED, EVENT_FAILED};
use crate::record::Owner;
use crate::route::{Route, Routes};
use crate::{InMemory, Memories, MemoryStore};

/// A second flip's target, or the store a route is replaced with mid-copy.
const ELSEWHERE: &str = "elsewhere";
/// Rounds of the two-thread race. The loser's lost swap is a window of a few
/// hundred nanoseconds; this many rounds all but certainly meet it.
const RACE_ROUNDS: usize = 200;
/// How long a race waits for a flip to answer before failing the round.
const PATIENCE: Duration = Duration::from_secs(5);
/// Threads swapping routes at once, and how many swaps each makes.
const SWAPPERS: u64 = 8;
const SWAPS: usize = 2_000;

/// A flip's verdict, reduced to what crosses a thread.
type Verdict = Result<usize, &'static str>;

/// A table over a source store that holds its first export until released.
fn held() -> (Arc<Rigged>, Memories) {
    let source = Arc::new(Rigged::new(SOURCE, None, Hold::Export));
    let store = Arc::<Rigged>::clone(&source) as Arc<dyn MemoryStore>;
    (source, Memories::over(unreachable_db(), store))
}

#[tokio::test]
async fn a_second_flip_is_refused_while_the_first_is_copying() {
    let (source, memories) = held();
    seeded(&source.inner).await;
    let workspace = id(WORKSPACE);
    let second = async {
        source.reached.notified().await;
        let refused = memories
            .flip(&workspace, Arc::new(InMemory::new(ELSEWHERE)))
            .await;
        source.release.notify_one();
        refused
    };

    let target = Arc::new(InMemory::new(TARGET));
    let (first, second) = tokio::join!(memories.flip(&workspace, target), second);

    let refused = second.expect_err("a workspace already copying takes no second flip");
    assert_eq!(
        (refused.code(), refused.detail()),
        (MEM_UNAVAILABLE, MOVING)
    );
    assert_eq!(first.expect("the first flip completes").copied, 9);
    let route = memories.routes().of(&workspace);
    assert_eq!(
        route.store.name(),
        TARGET,
        "the first flip's target is the store"
    );
}

#[tokio::test]
async fn a_flip_leaves_a_route_replaced_during_its_copy_in_place() {
    let (source, memories) = held();
    seeded(&source.inner).await;
    let workspace = id(WORKSPACE);
    let replacement = Arc::new(Route::settled(Arc::new(InMemory::new(ELSEWHERE))));
    let replace = async {
        source.reached.notified().await;
        let copying = memories.routes().of(&workspace);
        let replaced = memories.routes().swap(&workspace, &copying, &replacement);
        source.release.notify_one();
        replaced
    };

    let target = Arc::new(InMemory::new(TARGET));
    let (flipped, replaced) = tokio::join!(memories.flip(&workspace, target), replace);

    assert!(replaced, "the copying route was the one installed");
    let refused = flipped.expect_err("a flip whose route moved under it does not switch");
    assert_eq!(
        (refused.code(), refused.detail()),
        (MEM_UNAVAILABLE, MOVING)
    );
    assert!(
        Arc::ptr_eq(&memories.routes().of(&workspace), &replacement),
        "the route the flip did not install stays"
    );
}

#[tokio::test]
async fn a_flip_dropped_before_it_switches_puts_the_workspace_back_and_takes_the_next() {
    let (source, memories) = held();
    seeded(&source.inner).await;
    let workspace = id(WORKSPACE);
    let capture = Capture::install();
    let mut flip = Box::pin(memories.flip(&workspace, Arc::new(InMemory::new(TARGET))));
    let answered = tokio::select! {
        biased;
        flipped = &mut flip => Some(flipped),
        () = source.reached.notified() => None,
    };
    assert!(answered.is_none(), "the flip is held in its export");
    let copying = memories.routes().of(&workspace);
    assert!(
        copying.mirror.is_some(),
        "precondition: every write is mirrored"
    );

    drop(flip);

    let ended = capture.only(EVENT_FAILED);
    let logged = (ended.level, ended.field("error_code"));
    let code = INTERNAL_OPERATION_FAILED.as_str();
    assert_eq!(logged, (Level::WARN, Some(code)), "the drop ends the flip");
    let events = capture.events();
    let completed = events
        .iter()
        .filter(|e| e.field("event") == Some(EVENT_COMPLETED));
    assert_eq!(completed.count(), 0, "and nothing calls it complete");
    let route = memories.routes().of(&workspace);
    assert_eq!(route.store.name(), SOURCE, "the source is the store again");
    assert!(route.mirror.is_none(), "and no write is mirrored any more");
    let next = memories
        .flip(&workspace, Arc::new(InMemory::new(ELSEWHERE)))
        .await;
    assert_eq!(next.expect("the next flip is taken").copied, 9);
}

#[tokio::test]
async fn a_flipping_write_reaches_the_store_being_left_before_the_store_being_filled() {
    let source = Arc::new(InMemory::new(SOURCE));
    let target = Arc::new(Rigged::new(TARGET, None, Hold::Upsert));
    let left = Arc::<InMemory>::clone(&source) as Arc<dyn MemoryStore>;
    let memories = Memories::over(unreachable_db(), Arc::clone(&left));
    let (workspace, fleet) = (id(WORKSPACE), id(FLEETS[0]));
    let copying = Arc::new(Route {
        store: left,
        mirror: Some(Arc::<Rigged>::clone(&target) as Arc<dyn MemoryStore>),
    });
    let settled = memories.routes().of(&workspace);
    assert!(memories.routes().swap(&workspace, &settled, &copying));
    let order: Vec<_> = copying.writers().map(|store| store.name()).collect();
    assert_eq!(
        order,
        [SOURCE, TARGET],
        "a write catching up keeps the order"
    );
    let owner = Owner {
        workspace: &workspace,
        fleet: &fleet,
    };
    let pushed = delta("k00", Visibility::Fleet);
    let entries = [&pushed];
    let watch = async {
        target.reached.notified().await;
        let held_first = rows(source.as_ref()).await.len();
        target.release.notify_one();
        held_first
    };

    let at = UnixMillis::from_millis(PUSHED_AT);
    let (written, held_first) = tokio::join!(memories.upsert_through(owner, &entries, at), watch);

    written.expect("the write is taken");
    assert_eq!(held_first, 1, "the store being left held it first");
    assert_eq!(
        rows(&target.inner).await.len(),
        1,
        "then the one being filled"
    );
}

/// Two threads flip one workspace from one barrier; the winner is held in its
/// copy until the loser has answered.
fn race(memories: &Memories, source: &Rigged) -> (Option<Verdict>, Option<Verdict>) {
    let (workspace, start) = (id(WORKSPACE), Barrier::new(2));
    let (answered, answers) = mpsc::channel::<Verdict>();
    thread::scope(|scope| {
        for name in [TARGET, ELSEWHERE] {
            let (answered, start, workspace) = (answered.clone(), &start, &workspace);
            scope.spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .build()
                    .expect("a runtime per racer");
                start.wait();
                let to = Arc::new(InMemory::new(name));
                let flipped = runtime.block_on(memories.flip(workspace, to));
                let verdict = flipped.map(|done| done.copied).map_err(|e| e.detail());
                answered
                    .send(verdict)
                    .expect("the round outlives its racers");
            });
        }
        drop(answered);
        let first = answers.recv_timeout(PATIENCE).ok();
        // Held once: a broken guard that let both in holds only one, and the
        // other answers first.
        source.release.notify_one();
        (first, answers.recv_timeout(PATIENCE).ok())
    })
}

#[test]
fn two_flips_of_one_workspace_racing_on_two_threads_let_exactly_one_win() {
    // The pool under each table spawns its upkeep onto the runtime it meets.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime for the pools");
    let _pools = runtime.enter();
    for round in 0..RACE_ROUNDS {
        let (source, memories) = held();

        let (first, second) = race(&memories, &source);

        assert_eq!(
            first,
            Some(Err(MOVING)),
            "round {round}: the loser answers first"
        );
        assert_eq!(second, Some(Ok(0)), "round {round}: the winner copies");
    }
}

#[test]
fn route_swaps_for_many_workspaces_at_once_lose_no_update() {
    let routes = Routes::new(Arc::new(InMemory::new(SOURCE)));
    let start = Barrier::new(usize::try_from(SWAPPERS).expect("a small count"));
    let store: Arc<dyn MemoryStore> = Arc::new(InMemory::new(TARGET));

    let last: Vec<(Uuid7, Arc<Route>)> = thread::scope(|scope| {
        let swappers: Vec<_> = (0..SWAPPERS)
            .map(|nth| {
                let (routes, start, store) = (&routes, &start, &store);
                scope.spawn(move || {
                    let workspace = id(&format!("01990000-0000-7000-8000-{nth:012x}"));
                    start.wait();
                    let installed = (0..SWAPS).fold(routes.of(&workspace), |current, _| {
                        let next = Arc::new(Route::settled(Arc::clone(store)));
                        let swapped = routes.swap(&workspace, &current, &next);
                        assert!(swapped, "only this thread moves its workspace");
                        next
                    });
                    (workspace, installed)
                })
            })
            .collect();
        swappers
            .into_iter()
            .map(|swapper| swapper.join().expect("a swapper finishes"))
            .collect()
    });

    for (workspace, installed) in &last {
        let kept = Arc::ptr_eq(&routes.of(workspace), installed);
        assert!(kept, "{} keeps its last swap", workspace.as_str());
    }
}
