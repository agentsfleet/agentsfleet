//! One synthesized root span per daemon verb, timed live by a stage clock.
//!
//! ```text
//!   start ──admit──► mark ──announce──► mark ··· drop ─► one span
//!   └── each stage is the time since the previous mark; only reached ones ──┘
//! ```
//!
//! A verb builds a [`VerbClock`] at its entry and marks each stage boundary it
//! reaches. When the clock drops, on every exit, it records one root span from
//! its start to that moment, as [`crate::delivery`] does: through the global
//! tracer, asked for at record time, so a clock built before the exporter was
//! installed still reaches it.
//!
//! The stage type is a closed enum per verb, so a free-form key cannot be
//! written, and the only strings a span carries are the event and fleet
//! identifiers. A verb that reaches no stage records nothing: an offer pass
//! that found no work, or a request refused before its first boundary, has
//! nothing to place, and its request span already says it ran. A verb can
//! also give up a span it started with [`VerbClock::discard`].

use std::fmt::Debug;
use std::time::{Duration, Instant, SystemTime};

use opentelemetry::trace::{Span as _, SpanBuilder, SpanKind, Tracer};
use opentelemetry::{Context, KeyValue};

use crate::semconv;

#[cfg(test)]
mod tests;

/// One verb's stages: which span they belong to, and each one's key.
pub trait Stage: Copy + Eq + Debug + Send {
    /// The span this verb records.
    const SPAN: &'static str;
    /// The attribute this stage is written under.
    fn key(self) -> &'static str;
}

/// Declares one verb's closed stage enum beside its span name and keys.
macro_rules! stages {
    ($(#[$doc:meta])* $name:ident, $span:path { $($(#[$sdoc:meta])* $stage:ident => $key:path),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            $($(#[$sdoc])* $stage),+
        }

        impl Stage for $name {
            const SPAN: &'static str = $span;
            fn key(self) -> &'static str {
                match self {
                    $(Self::$stage => $key),+
                }
            }
        }
    };
}

stages!(
    /// A steer, from its extractors to the admitted announce.
    SteerStage, semconv::SPAN_FLEET_STEER {
        /// The steering append.
        Admit => semconv::ATTR_STEER_ADMIT_MS,
        /// The admitted announce.
        Announce => semconv::ATTR_STEER_ANNOUNCE_MS,
    }
);

stages!(
    /// An offer that selected work, from the claim to the stream write.
    OfferStage, semconv::SPAN_FLEET_OFFER {
        /// Selection and the slot claim.
        Claim => semconv::ATTR_OFFER_CLAIM_MS,
        /// Delivery, payer, provider and gates.
        Admit => semconv::ATTR_OFFER_ADMIT_MS,
        /// The policy and the issue commit.
        Issue => semconv::ATTR_OFFER_ISSUE_MS,
        /// The offer frame.
        Encode => semconv::ATTR_OFFER_ENCODE_MS,
        /// The stream write.
        Send => semconv::ATTR_OFFER_SEND_MS,
    }
);

stages!(
    /// A terminal report, from the lease read to the acknowledgement.
    ReportStage, semconv::SPAN_FLEET_REPORT {
        /// The lease read.
        Load => semconv::ATTR_REPORT_LOAD_MS,
        /// The task report and pricing.
        Prepare => semconv::ATTR_REPORT_PREPARE_MS,
        /// The inline memory capture.
        Capture => semconv::ATTR_REPORT_CAPTURE_MS,
        /// The report's commit.
        Commit => semconv::ATTR_REPORT_COMMIT_MS,
        /// The completion announce.
        Announce => semconv::ATTR_REPORT_ANNOUNCE_MS,
        /// The acknowledgement write.
        Ack => semconv::ATTR_REPORT_ACK_MS,
    }
);

stages!(
    /// A fallback memory hydrate.
    HydrateStage, semconv::SPAN_FLEET_HYDRATE {
        /// The fence check.
        Fence => semconv::ATTR_HYDRATE_FENCE_MS,
        /// The memory read.
        MemoryRead => semconv::ATTR_HYDRATE_MEMORY_READ_MS,
    }
);

stages!(
    /// A memory capture.
    CaptureStage, semconv::SPAN_FLEET_CAPTURE {
        /// The fence check.
        Fence => semconv::ATTR_CAPTURE_FENCE_MS,
        /// The memory write.
        MemoryWrite => semconv::ATTR_CAPTURE_MEMORY_WRITE_MS,
    }
);

/// Where a clock reads its instants: the host's clocks, or a test's.
pub trait Ticks: Debug + Send {
    /// The monotonic instant stages are measured on.
    fn monotonic(&self) -> Instant;
    /// The wall instant the span is placed at.
    fn wall(&self) -> SystemTime;
}

/// The host's clocks.
#[derive(Debug, Clone, Copy, Default)]
pub struct HostTicks;

impl Ticks for HostTicks {
    fn monotonic(&self) -> Instant {
        Instant::now()
    }

    fn wall(&self) -> SystemTime {
        af_core::clock::now().into()
    }
}

/// The most stages any verb has, so a clock never reallocates.
const MOST_STAGES: usize = 6;

/// One verb's stage clock; it records its span when dropped.
#[derive(Debug)]
pub struct VerbClock<S: Stage, C: Ticks = HostTicks> {
    ticks: C,
    started_at: SystemTime,
    started: Instant,
    last: Instant,
    stages: Vec<(S, Duration)>,
    event_id: Option<String>,
    agent_id: Option<String>,
    discarded: bool,
}

impl<S: Stage> VerbClock<S> {
    /// A clock started now on the host's clocks.
    #[must_use]
    pub fn start() -> Self {
        Self::start_on(HostTicks)
    }

    /// A clock that records nothing, for a caller that times no verb.
    #[must_use]
    pub fn untimed() -> Self {
        let mut clock = Self::start();
        clock.discard();
        clock
    }
}

impl<S: Stage, C: Ticks> VerbClock<S, C> {
    /// A clock started now on `ticks`.
    #[must_use]
    pub fn start_on(ticks: C) -> Self {
        let started = ticks.monotonic();
        Self {
            started_at: ticks.wall(),
            started,
            last: started,
            ticks,
            stages: Vec::with_capacity(MOST_STAGES),
            event_id: None,
            agent_id: None,
            discarded: false,
        }
    }

    /// `stage` ends here: it took the time since the previous mark. A stage
    /// already marked keeps its first reading, so each is written once.
    pub fn mark(&mut self, stage: S) {
        let now = self.ticks.monotonic();
        if self.stages.iter().any(|(marked, _)| *marked == stage) {
            return;
        }
        self.stages
            .push((stage, now.saturating_duration_since(self.last)));
        self.last = now;
    }

    /// The event this verb served and the fleet it belongs to.
    pub fn identify(&mut self, event_id: &str, agent_id: &str) {
        self.event_id = Some(event_id.to_owned());
        self.agent_id = Some(agent_id.to_owned());
    }

    /// The fleet this verb served, where no event is known.
    pub fn agent(&mut self, agent_id: &str) {
        self.agent_id = Some(agent_id.to_owned());
    }

    /// Records the span so far and starts a fresh one in its place, timed or
    /// untimed as this clock is: one pass that serves a second offer.
    pub fn restart(&mut self) {
        if self.discarded {
            self.reset();
        } else {
            self.restart_on(&opentelemetry::global::tracer(semconv::SCOPE_NAME));
        }
    }

    /// [`Self::restart`] against a tracer the caller names, for a clock that
    /// records: the span so far is closed, and the clock records again.
    fn restart_on<T: Tracer>(&mut self, tracer: &T) {
        self.record_on(tracer);
        self.discarded = false;
        self.reset();
    }

    /// [`Self::restart`], but only once a stage has been marked: a clock that
    /// reached nothing has no span to close, so it keeps its start and identity.
    pub fn restart_if_staged(&mut self) {
        if !self.stages.is_empty() {
            self.restart();
        }
    }

    /// Back to a fresh start, keeping whether this clock records.
    fn reset(&mut self) {
        let now = self.ticks.monotonic();
        self.started_at = self.ticks.wall();
        self.started = now;
        self.last = now;
        self.stages.clear();
        self.event_id = None;
        self.agent_id = None;
    }

    /// This verb has nothing to describe; dropping records no span.
    pub fn discard(&mut self) {
        self.discarded = true;
    }

    /// Records the span on `tracer` now, once.
    fn record_on<T: Tracer>(&mut self, tracer: &T) {
        if std::mem::replace(&mut self.discarded, true) {
            return;
        }
        // No stage, no span: see the module note.
        if self.stages.is_empty() {
            return;
        }
        let elapsed = self
            .ticks
            .monotonic()
            .saturating_duration_since(self.started);
        let ended_at = self
            .started_at
            .checked_add(elapsed)
            .unwrap_or(self.started_at);
        // An empty context: the verb's request or stream span is not its
        // parent, as for fleet.delivery, and a root joins by attribute.
        let mut span = SpanBuilder::from_name(S::SPAN)
            .with_kind(SpanKind::Internal)
            .with_start_time(self.started_at)
            .with_attributes(self.attributes())
            .start_with_context(tracer, &Context::new());
        span.end_with_timestamp(ended_at);
    }

    /// The identity, then each reached stage in whole milliseconds.
    fn attributes(&self) -> Vec<KeyValue> {
        let identity = [
            (semconv::ATTR_EVENT_ID, &self.event_id),
            (semconv::ATTR_AGENT_ID, &self.agent_id),
        ];
        let named = identity
            .into_iter()
            .filter_map(|(key, value)| Some(KeyValue::new(key, value.clone()?)));
        let staged = self.stages.iter().map(|(stage, took)| {
            KeyValue::new(stage.key(), af_core::clock::saturating_millis_signed(*took))
        });
        named.chain(staged).collect()
    }
}

impl<S: Stage, C: Ticks> Drop for VerbClock<S, C> {
    fn drop(&mut self) {
        // Checked first, so an untimed or discarded clock never reads the
        // global provider.
        if !self.discarded {
            self.record_on(&opentelemetry::global::tracer(semconv::SCOPE_NAME));
        }
    }
}
