//! Where a run's time went, read off the frames it emitted.
//!
//! The sink sees every frame the run emits, and two of them bracket a tool
//! call, so the run's tool time is summed here without a hook into the engine:
//! a completed call's own `ms`, and for a call cut before it completed, the
//! time from its start frame to the run's end. The model's time is what is
//! left of the run once its tools are taken out.
//!
//! The clock starts when the run is started, not when the sink is made: the
//! sink is made before the bundle, the memory and the sandbox are ready, and a
//! first token measured from there read setup as model time.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use afd_wire::activity::ActivityFrame;
use tokio::time::Instant;

/// A run's elapsed time, split where the frames say it went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RunTiming {
    /// From the run's start to its end.
    pub(crate) elapsed: Duration,
    /// Inside `elapsed`: every tool call, a cut one counted to the end.
    pub(crate) tools: Duration,
    /// Inside `elapsed`: the first answer chunk, when one came.
    pub(crate) first_chunk: Option<Duration>,
}

impl RunTiming {
    /// What the model took: the run less its tools.
    pub(crate) fn model(self) -> Duration {
        self.elapsed.saturating_sub(self.tools)
    }
}

/// The clock a run's frames are read against.
#[derive(Debug)]
pub(crate) struct RunClock {
    /// When the sink was made; the run's start replaces it.
    made: Instant,
    inner: Mutex<Clocked>,
}

/// What the clock has seen, under one lock the emit path holds briefly.
#[derive(Debug, Default)]
struct Clocked {
    started: Option<Instant>,
    first_chunk: Option<Duration>,
    /// Calls started and not yet completed, by the id their frames share.
    open: HashMap<String, Instant>,
    /// Every completed call's own duration.
    completed: Duration,
}

impl RunClock {
    pub(crate) fn new() -> Self {
        Self {
            made: Instant::now(),
            inner: Mutex::new(Clocked::default()),
        }
    }

    /// The run starts now: the first token and the tools are measured from
    /// here. A sink nobody started measures from when it was made.
    pub(crate) fn start(&self) {
        let mut clocked = self.lock();
        clocked.started.get_or_insert_with(Instant::now);
    }

    /// Reads one frame as it goes by.
    pub(crate) fn note(&self, frame: &ActivityFrame<'_>) {
        let mut clocked = self.lock();
        let since = clocked.started.unwrap_or(self.made);
        match frame {
            ActivityFrame::FleetResponseChunk(_) => {
                clocked.first_chunk.get_or_insert_with(|| since.elapsed());
            }
            ActivityFrame::ToolCallStarted(body) => {
                if let Some(id) = body.call_id.as_deref() {
                    clocked.open.insert(id.to_owned(), Instant::now());
                }
            }
            ActivityFrame::ToolCallCompleted(body) => {
                if let Some(id) = body.call_id.as_deref() {
                    clocked.open.remove(id);
                }
                let ms = u64::try_from(body.ms).unwrap_or_default();
                clocked.completed += Duration::from_millis(ms);
            }
            ActivityFrame::ToolCallProgress(_) => {}
        }
    }

    /// The run's timing as of now; a call still open is counted to here.
    pub(crate) fn read(&self) -> RunTiming {
        let clocked = self.lock();
        let since = clocked.started.unwrap_or(self.made);
        let cut: Duration = clocked.open.values().map(Instant::elapsed).sum();
        RunTiming {
            elapsed: since.elapsed(),
            tools: clocked.completed + cut,
            first_chunk: clocked.first_chunk,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Clocked> {
        // A frame's bookkeeping cannot panic while the lock is held, so the
        // clock is never poisoned; a poisoned one would still carry its reads.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
