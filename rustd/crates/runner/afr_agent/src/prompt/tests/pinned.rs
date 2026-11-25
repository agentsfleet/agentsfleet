//! The fixed text names the fleet, and only its own `core` entries follow it
//! under one heading.

use std::borrow::Cow;

use af_wire::memory::{MemoryDelta, SharedMemory, Visibility};
use afr_memory::Seed;

use super::super::Prompt;
use crate::fixture::{payload, unbounded};

/// The fixed text, for the fixture fleet `support-triage`.
const FIXED: &str = "You are support-triage, a fleet in agentsfleet: a long-lived agent a team talks to in one chat.
Remember standing guidance with memory_store under the category core, only when the person's newest message asks you to. It is shown to you at the start of every run. Store anything else under another category.
Schedule yourself with the cron tools, add a webhook, GitHub or Slack trigger with trigger_add, and start work that must keep going with task_start, each only when the person's newest message asks.
You hold only the secrets and integrations a person approved for you. When you need one, call request_access with its name (and, for GitHub, the repositories and read or write); the person approves it once.";

/// One of the fleet's own entries.
fn own(key: &'static str, content: &'static str, category: &'static str) -> MemoryDelta<'static> {
    MemoryDelta {
        key: Cow::Borrowed(key),
        content: Cow::Borrowed(content),
        category: Cow::Borrowed(category),
        visibility: Visibility::Fleet,
    }
}

/// The prompt of the fixture run whose memory load carried `window` and
/// `shared`.
fn prompted(window: &[MemoryDelta<'_>], shared: &[SharedMemory<'_>]) -> Prompt {
    let memory = Seed {
        window,
        shared,
        publish: false,
        recall: None,
    };
    Prompt::new(&payload(&[], unbounded()), &memory)
}

/// A run whose load carried two pinned entries is told who it is, then both
/// entries under one heading, newest first; the compaction digest's text is
/// the fixed text alone.
#[test]
fn a_runs_prompt_carries_the_fleets_name_and_pinned_memory() {
    let window = [
        own("evidence", "Always link the evidence.", "core"),
        own(
            "grafana",
            "You only read; query Grafana at grafana.example.",
            "core",
        ),
    ];

    let prompt = prompted(&window, &[]);

    assert_eq!(
        prompt.instructions,
        format!(
            "{FIXED}\n\n## Pinned memory\n- evidence: Always link the evidence.\n\
             - grafana: You only read; query Grafana at grafana.example."
        )
    );
    assert_eq!(prompt.fixed, FIXED);
}

/// A note, an entry stored with no category, and another fleet's shared
/// `core` entry stay recall-only: none reaches the prompt, and with nothing
/// pinned the heading is absent too.
#[test]
fn a_prompt_leaves_out_unpinned_and_shared_memory() {
    let window = [
        own("standup", "The standup moved to 10:00.", "note"),
        own(
            "deploy",
            "Staging deploys first.",
            guffbolt::tools::memory::DEFAULT_CATEGORY,
        ),
    ];
    let shared = [SharedMemory {
        key: Cow::Borrowed("escalation"),
        content: Cow::Borrowed("Page the on-call engineer."),
        category: Cow::Borrowed("core"),
        writer_fleet_id: Cow::Borrowed("0190d5c2-7e1a-7000-8000-00000000d0d4"),
        writer_fleet_name: Cow::Borrowed("incident-desk"),
        updated_at: jiff::Timestamp::UNIX_EPOCH,
    }];

    let prompt = prompted(&window, &shared);

    assert_eq!(prompt.instructions, FIXED);
    for left_out in ["standup", "Staging", "escalation", "## Pinned memory"] {
        assert!(!prompt.instructions.contains(left_out), "{left_out}");
    }
}

/// Pinned entries past the load's byte budget stay out, newest kept first:
/// the block is bounded by the window that bounds the payload.
#[test]
fn pinned_memory_past_the_load_budget_stays_out() {
    let large = "x".repeat(af_wire::memory::LOAD_WINDOW_BYTES - 16);
    let window = [
        MemoryDelta {
            key: Cow::Borrowed("first"),
            content: Cow::Owned(large),
            category: Cow::Borrowed("core"),
            visibility: Visibility::Fleet,
        },
        own("second", "Past the budget.", "core"),
    ];

    let prompt = prompted(&window, &[]);

    assert!(prompt.instructions.contains("- first: xxx"));
    assert!(!prompt.instructions.contains("second"));
}
