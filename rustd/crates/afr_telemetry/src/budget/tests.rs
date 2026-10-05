//! The span budget: per lease, per second, and the slot a lease gives back.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};

use opentelemetry::Context;
use opentelemetry::trace::{
    Span as _, TraceContextExt as _, TraceId, Tracer as _, TracerProvider as _,
};
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{SdkTracerProvider, SpanData, SpanExporter};

use super::table::{LeaseTable, SecondWindow};
use super::{
    LeaseSampler, Limits, MAX_LEASE_SPANS, Monotonic, RUNNER_SPANS_PER_SECOND, Seconds,
    TRACKED_LEASES, key,
};

/// The instrumentation scope this suite records under.
const SCOPE: &str = "a-test";

/// A clock a test moves by hand.
#[derive(Debug, Clone, Default)]
struct Hand(Arc<AtomicU32>);

impl Seconds for Hand {
    fn second(&self) -> u32 {
        self.0.load(Ordering::SeqCst)
    }
}

/// An exporter that counts what reached it.
#[derive(Debug, Clone, Default)]
struct Kept(Arc<AtomicUsize>);

impl SpanExporter for Kept {
    fn export(&self, batch: Vec<SpanData>) -> impl Future<Output = OTelSdkResult> + Send {
        self.0.fetch_add(batch.len(), Ordering::SeqCst);
        std::future::ready(Ok(()))
    }
}

/// A pipeline under `limits`, what it kept, what it shed, and its clock.
struct Pipeline {
    provider: SdkTracerProvider,
    sampler: LeaseSampler,
    kept: Arc<AtomicUsize>,
    shed: Arc<AtomicU64>,
    clock: Hand,
}

impl Pipeline {
    fn new(limits: Limits) -> Self {
        let clock = Hand::default();
        let shed = Arc::new(AtomicU64::new(0));
        let counted = Arc::clone(&shed);
        let sampler = LeaseSampler::new(limits, clock.clone(), move || {
            counted.fetch_add(1, Ordering::SeqCst);
        });
        let kept = Kept::default();
        let provider = SdkTracerProvider::builder()
            .with_sampler(sampler.clone())
            .with_span_processor(sampler.reaper())
            .with_simple_exporter(kept.clone())
            .build();
        Self {
            provider,
            sampler,
            kept: kept.0,
            shed,
            clock,
        }
    }

    /// Starts a lease's root span, returning the context its children start in.
    fn lease(&self) -> Context {
        let root = self.provider.tracer(SCOPE).start("runner.lease");
        Context::current_with_span(root)
    }

    /// Starts and ends `count` children of `lease`.
    fn children(&self, lease: &Context, count: u32) {
        let tracer = self.provider.tracer(SCOPE);
        for _call in 0..count {
            tracer.start_with_context("execute_tool", lease).end();
        }
    }

    fn kept(&self) -> usize {
        self.kept.load(Ordering::SeqCst)
    }

    fn shed(&self) -> u64 {
        self.shed.load(Ordering::SeqCst)
    }

    fn held(&self) -> usize {
        self.sampler.budget.leases.held()
    }
}

/// A lease past `MAX_LEASE_SPANS` exports exactly that many, root included,
/// and counts the rest; its slot is free once its root ends.
#[test]
fn test_lease_spans_stop_at_the_budget() {
    let pipeline = Pipeline::new(Limits {
        per_second: u32::MAX,
        ..Limits::default()
    });
    let lease = pipeline.lease();
    assert_eq!(pipeline.held(), 1, "the root claimed its lease's slot");

    // The root is one span, so this is the budget plus ten.
    pipeline.children(&lease, MAX_LEASE_SPANS + 9);
    lease.span().end();

    assert_eq!(
        pipeline.kept(),
        MAX_LEASE_SPANS as usize,
        "exactly the budget leaves"
    );
    assert_eq!(pipeline.shed(), 10, "and every span past it is counted");
    assert_eq!(pipeline.held(), 0, "the ended root gave its slot back");
}

/// A burst past the per-second budget is shed and counted, and the next
/// second admits again; a shed child costs its lease nothing.
#[test]
fn test_span_budget_refills_each_second() {
    let pipeline = Pipeline::new(Limits {
        per_lease: u32::MAX,
        ..Limits::default()
    });
    let lease = pipeline.lease();

    pipeline.children(&lease, RUNNER_SPANS_PER_SECOND + 5);
    assert_eq!(pipeline.shed(), 5, "the burst past the second is shed");
    assert_eq!(
        pipeline.kept(),
        RUNNER_SPANS_PER_SECOND as usize,
        "the second admitted its budget; the root waits for its end"
    );

    pipeline.clock.0.store(1, Ordering::SeqCst);
    pipeline.children(&lease, 1);
    assert_eq!(pipeline.shed(), 5, "the next second admits again");
    lease.span().end();
    assert_eq!(pipeline.kept(), RUNNER_SPANS_PER_SECOND as usize + 2);
}

/// Two leases are budgeted apart: one spending its whole budget leaves the
/// other's untouched.
#[test]
fn two_leases_are_budgeted_apart() {
    let pipeline = Pipeline::new(Limits {
        per_lease: 3,
        per_second: u32::MAX,
    });
    let first = pipeline.lease();
    let second = pipeline.lease();

    pipeline.children(&first, 5);
    pipeline.children(&second, 2);

    assert_eq!(pipeline.shed(), 3, "only the first lease went over");
    first.span().end();
    second.span().end();
    assert_eq!(pipeline.kept(), 3 + 3);
    assert_eq!(pipeline.held(), 0);
}

/// A full table keeps every root and sheds the untracked lease's children,
/// so a leak bounds itself rather than growing.
#[test]
fn a_full_table_keeps_the_root_and_sheds_its_children() {
    let pipeline = Pipeline::new(Limits::default());
    let leases: Vec<Context> = (0..TRACKED_LEASES).map(|_lease| pipeline.lease()).collect();
    assert_eq!(pipeline.held(), TRACKED_LEASES);

    let untracked = pipeline.lease();
    pipeline.children(&untracked, 2);
    untracked.span().end();

    assert_eq!(
        pipeline.shed(),
        2,
        "the untracked lease's children are shed"
    );
    assert_eq!(pipeline.kept(), 1, "its root is kept all the same");
    for lease in leases {
        lease.span().end();
    }
    assert_eq!(pipeline.held(), 0);
    let flushed = pipeline.provider.shutdown();
    assert!(
        flushed.is_ok(),
        "the reaper flushes and shuts down cleanly: {flushed:?}"
    );
}

/// The window counts a lagging second against the newer one, and a zero
/// budget admits nothing.
#[test]
fn the_window_never_runs_backwards() {
    let window = SecondWindow::default();
    assert!(window.take(5, 1));
    assert!(
        !window.take(4, 1),
        "second 4 arrived late and counts against 5"
    );
    assert!(window.take(6, 1));
    assert!(!SecondWindow::default().take(0, 0));
}

/// A lease table finds, frees and refills a slot by key.
#[test]
fn a_slot_is_found_freed_and_reused() {
    let table = LeaseTable::with_capacity(2);
    assert!(table.open(7));
    assert!(table.find(7).is_some());
    table.close(7);
    assert!(table.find(7).is_none());
    assert!(table.open(9) && table.open(11));
    assert!(!table.open(13), "both slots are held");
    let slot = table.find(9).expect("9 holds a slot");
    assert!(
        slot.reserve(2),
        "its root counted one; the limit admits a second"
    );
    assert!(!slot.reserve(2));
    slot.unreserve();
    assert!(slot.reserve(2));
}

/// A trace's key is never the vacant word, even for the invalid trace.
#[test]
fn a_key_is_never_vacant() {
    assert_eq!(key(TraceId::INVALID), 1);
    assert_ne!(key(TraceId::from(42_u128)), 0);
}

/// The production clock starts at second zero, and the sampler renders
/// without its closures.
#[test]
fn the_production_clock_starts_at_zero_and_the_sampler_renders() {
    assert_eq!(Monotonic::start().second(), 0);
    let sampler = LeaseSampler::new(Limits::default(), Monotonic::start(), || {});
    let rendered = format!("{sampler:?} {:?}", sampler.reaper());
    assert!(rendered.contains("per_lease"), "{rendered}");
}

/// Workers released together onto one lease, as a burst of tool calls lands.
const WORKERS: usize = 128;

/// Calls each worker makes.
const CALLS_PER_WORKER: u32 = 4;

/// Runs `CALLS_PER_WORKER` children on each of `WORKERS` threads, all released
/// off one barrier, under one lease of `pipeline`; ends the lease after.
fn contend(pipeline: &Pipeline) {
    let lease = pipeline.lease();
    let barrier = Barrier::new(WORKERS);
    std::thread::scope(|scope| {
        for _worker in 0..WORKERS {
            scope.spawn(|| {
                barrier.wait();
                pipeline.children(&lease, CALLS_PER_WORKER);
            });
        }
    });
    lease.span().end();
}

/// Every span started, the root included.
fn started() -> u64 {
    u64::try_from(WORKERS).unwrap_or(u64::MAX) * u64::from(CALLS_PER_WORKER) + 1
}

/// A lease under real contention keeps exactly its budget and counts every
/// other span: no reservation lost to a race, none counted twice.
///
/// Repeated, because a lost compare-and-swap shows up as an off-by-some only
/// on the run where two workers collide.
#[test]
fn contended_children_never_overrun_a_lease() {
    for _run in 0..5 {
        let pipeline = Pipeline::new(Limits {
            per_lease: 64,
            per_second: u32::MAX,
        });

        contend(&pipeline);

        assert_eq!(pipeline.kept(), 64, "exactly the lease's budget left");
        assert_eq!(
            u64::try_from(pipeline.kept()).unwrap_or(u64::MAX) + pipeline.shed(),
            started(),
            "every span is either kept or counted"
        );
        assert_eq!(pipeline.held(), 0);
    }
}

/// The per-second window under the same contention admits exactly its
/// budget, and a shed child hands its lease reservation back.
#[test]
fn contended_children_never_overrun_the_second() {
    for _run in 0..5 {
        let pipeline = Pipeline::new(Limits {
            per_lease: u32::MAX,
            per_second: 100,
        });

        contend(&pipeline);

        assert_eq!(
            pipeline.kept(),
            100 + 1,
            "the second's budget, and the root"
        );
        assert_eq!(
            u64::try_from(pipeline.kept()).unwrap_or(u64::MAX) + pipeline.shed(),
            started()
        );
    }
}

/// Every lease a runner at its worker ceiling holds at once is tracked, so
/// none of their children is shed as untracked.
#[test]
fn every_lease_a_full_runner_holds_is_tracked() {
    let pipeline = Pipeline::new(Limits::default());
    let workers = usize::try_from(afd_core::limits::MAX_WORKERS).unwrap_or(usize::MAX);
    let leases: Vec<Context> = (0..workers).map(|_lease| pipeline.lease()).collect();

    for lease in &leases {
        pipeline.children(lease, 1);
    }

    assert_eq!(pipeline.shed(), 0, "a full runner's every lease has a slot");
    assert!(TRACKED_LEASES >= workers);
    for lease in leases {
        lease.span().end();
    }
}

/// Two traces sharing their low half get different keys: the key folds both
/// halves, so two leases never share a slot by sharing half an id.
#[test]
fn traces_differing_only_in_their_high_half_get_different_keys() {
    let low = 5_u128;
    let first = TraceId::from((1_u128 << 64) | low);
    let second = TraceId::from((2_u128 << 64) | low);

    assert_ne!(key(first), key(second));
}
