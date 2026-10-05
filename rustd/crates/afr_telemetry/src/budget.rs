//! A fixed span budget: so many spans per lease, so many per second.
//!
//! ```text
//!   span starts ─► LeaseSampler::should_sample
//!                    │ no parent: a lease's root ─► claim a slot, charge the second; always kept
//!                    │ a parent that was shed    ─► shed, counted (its trace has a hole above it)
//!                    │ a kept parent: its lease's slot ─► per-lease count < MAX_LEASE_SPANS?
//!                    │                                   per-second window < RUNNER_SPANS_PER_SECOND?
//!                    │                                   both ─► kept   either not ─► shed, counted
//!   root span ends ─► LeaseReaper::ended ─► the slot is free for the next lease
//! ```
//!
//! # Why the root is always kept
//!
//! Each lease is its own trace, joined to the daemon's `fleet.delivery` span by
//! its `agentsfleet.lease.id` and `agentsfleet.event.id` attributes. A trace
//! whose root was shed is a pile of children an operator cannot find, so the
//! root spends one of its lease's spans and is never refused. It is charged to
//! the second all the same, so the children that second admits make room for
//! it. What bounds roots is the rate leases start at, not this budget: one per
//! lease, and a runner holds at most `MAX_WORKERS` leases at a time.
//!
//! # Why a shed span's children are shed
//!
//! The SDK keeps a span it dropped as the active parent of what starts inside
//! it. Admitting those children would export spans pointing at a parent the
//! collector never received — an orphaned subtree that spends its lease's
//! budget and joins nothing. So a child of a span that was not sampled is shed
//! and counted like any other.
//!
//! # Why atomics and no lock
//!
//! The sampler runs on every span start on every worker. The lease table is a
//! fixed array of slots claimed by compare-and-swap, and the window is one
//! packed word, so admitting a span never waits on another worker and never
//! allocates. A full table, which a leak would cause, sheds children rather
//! than growing.

use std::sync::Arc;
use std::time::Instant;

use afd_core::limits::MAX_WORKERS;
use afd_otlp::SpanEnd;
use opentelemetry::trace::{Link, SpanKind, TraceContextExt as _, TraceId, TraceState};
use opentelemetry::{Context, KeyValue};
use opentelemetry_sdk::trace::{SamplingDecision, SamplingResult, ShouldSample, SpanData};

use self::table::{LeaseTable, SecondWindow};

mod table;

#[cfg(test)]
mod tests;

/// Spans one lease may export, its root included: the root and 255 children.
///
/// Sized from the heaviest lease the suites drive — the repair bundle's run,
/// a root, its `invoke_agent`, and a `chat` and an `execute_tool` per step,
/// under forty spans — with room for a lease many times longer. A model looping
/// on one tool reaches it, and the rest of that lease is counted, not sent.
pub const MAX_LEASE_SPANS: u32 = 256;

/// Spans the whole runner may export per monotonic second, roots included.
///
/// The batch processor holds [`afd_otlp::SPAN_QUEUE`] spans and sends every
/// [`afd_otlp::SPAN_SEND_EVERY`], both pinned, so 128 a second fills at most
/// 640 of its 2048: against a collector that keeps up, the queue never drops
/// a span the budget admitted. A collector slower than the budget is a
/// different matter — its failed exports are counted in
/// `agentsfleet_runner_otlp_entries_discarded_total`, and a queue it lets fill
/// drops inside the SDK, where nothing counts it.
pub const RUNNER_SPANS_PER_SECOND: u32 = 128;

/// Leases the table tracks at once: four times the most a runner can hold, so
/// a lease's slot is free long before the table could fill.
const TRACKED_LEASES: usize = MAX_WORKERS as usize * 4;

/// The two budgets, adjustable for a test that needs one out of the way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Spans one lease may export.
    pub per_lease: u32,
    /// Spans the runner may export per second.
    pub per_second: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            per_lease: MAX_LEASE_SPANS,
            per_second: RUNNER_SPANS_PER_SECOND,
        }
    }
}

/// Which monotonic second it is.
pub trait Seconds: Send + Sync + core::fmt::Debug {
    /// Whole seconds since an origin of the clock's choosing.
    fn second(&self) -> u32;
}

/// The process's monotonic clock, counted from when the sampler was built.
#[derive(Debug, Clone, Copy)]
pub struct Monotonic(Instant);

impl Monotonic {
    /// A clock whose second zero is now.
    #[must_use]
    pub fn start() -> Self {
        Self(Instant::now())
    }
}

impl Seconds for Monotonic {
    fn second(&self) -> u32 {
        u32::try_from(self.0.elapsed().as_secs()).unwrap_or(u32::MAX)
    }
}

/// Decides, for every runner span, whether it is exported.
///
/// A concrete type implementing the SDK's own sampling trait
/// (`M-DI-HIERARCHY`): the pipeline takes it as a `ShouldSample`, and the
/// budget behind it is shared with its [`LeaseReaper`].
#[derive(Debug, Clone)]
pub struct LeaseSampler {
    budget: Arc<Budget>,
}

impl LeaseSampler {
    /// A sampler holding `limits`, reading `clock`, calling `shed` for every
    /// span it refuses.
    #[must_use]
    pub fn new(
        limits: Limits,
        clock: impl Seconds + 'static,
        shed: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            budget: Arc::new(Budget {
                limits,
                leases: LeaseTable::with_capacity(TRACKED_LEASES),
                window: SecondWindow::default(),
                clock: Box::new(clock),
                shed: Box::new(shed),
            }),
        }
    }

    /// The processor that frees a lease's slot when its root span ends.
    #[must_use]
    pub fn reaper(&self) -> LeaseReaper {
        LeaseReaper {
            budget: Arc::clone(&self.budget),
        }
    }
}

impl ShouldSample for LeaseSampler {
    fn should_sample(
        &self,
        parent_context: Option<&Context>,
        trace_id: TraceId,
        _name: &str,
        _span_kind: &SpanKind,
        _attributes: &[KeyValue],
        _links: &[Link],
    ) -> SamplingResult {
        let parent = parent_context.filter(|context| context.has_active_span());
        let admitted = match parent {
            None => {
                self.budget.open(trace_id);
                true
            }
            Some(shed) if !shed.span().span_context().is_sampled() => self.budget.refuse(),
            Some(_kept) => self.budget.admit(trace_id),
        };
        SamplingResult {
            decision: if admitted {
                SamplingDecision::RecordAndSample
            } else {
                SamplingDecision::Drop
            },
            attributes: Vec::new(),
            trace_state: parent.map_or_else(TraceState::default, |context| {
                context.span().span_context().trace_state().clone()
            }),
        }
    }
}

/// Frees a lease's slot in the budget when the lease's root span ends.
///
/// An observer inside the pipeline's one processor rather than a processor of
/// its own, so the SDK moves each ended span once instead of cloning it for a
/// second processor (`afd_otlp::SpanEnd` says why).
#[derive(Debug)]
pub struct LeaseReaper {
    budget: Arc<Budget>,
}

impl SpanEnd for LeaseReaper {
    fn ended(&self, span: &SpanData) {
        if span.parent_span_id == opentelemetry::trace::SpanId::INVALID {
            self.budget.close(span.span_context.trace_id());
        }
    }
}

/// The state the sampler and its reaper share.
struct Budget {
    limits: Limits,
    leases: LeaseTable,
    window: SecondWindow,
    clock: Box<dyn Seconds>,
    shed: Box<dyn Fn() + Send + Sync>,
}

impl core::fmt::Debug for Budget {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Budget")
            .field("limits", &self.limits)
            .field("leases", &self.leases)
            .finish_non_exhaustive()
    }
}

impl Budget {
    /// A lease's root span started: its slot is claimed, its root counted
    /// against the lease and charged to the second. A full table leaves the
    /// root kept and its children untracked, which sheds them; a full second
    /// keeps the root anyway, because a root is never refused.
    fn open(&self, trace: TraceId) {
        let _claimed = self.leases.open(key(trace));
        let _charged = self
            .window
            .take(self.clock.second(), self.limits.per_second);
    }

    /// A child span started: kept when its lease and the second both have
    /// room. A per-lease reservation the second refused is handed back, so a
    /// lease is charged only for what it exported.
    fn admit(&self, trace: TraceId) -> bool {
        let admitted = match self.leases.find(key(trace)) {
            Some(lease) if lease.reserve(self.limits.per_lease) => {
                let fits = self
                    .window
                    .take(self.clock.second(), self.limits.per_second);
                if !fits {
                    lease.unreserve();
                }
                fits
            }
            // Untracked (the table was full when its root started) or spent.
            _refused => false,
        };
        if admitted { true } else { self.refuse() }
    }

    /// A span refused: counted as shed, and never admitted.
    fn refuse(&self) -> bool {
        (self.shed)();
        false
    }

    /// A lease's root span ended: its slot is free.
    fn close(&self, trace: TraceId) {
        self.leases.close(key(trace));
    }
}

/// The table key a trace is tracked under: its two halves folded, never zero.
fn key(trace: TraceId) -> u64 {
    let id = u128::from_be_bytes(trace.to_bytes());
    let folded = (id ^ (id >> 64)) & u128::from(u64::MAX);
    u64::try_from(folded).unwrap_or(u64::MAX).max(1)
}
