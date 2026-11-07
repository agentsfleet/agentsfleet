//! Paired operational traces exclude private history on every ending.
use afd_context::{Compaction, Group, Host, Identity, Result, Trace};
use afd_core::{error_code, test_util::trace::Capture};

fn task() -> Compaction {
    Compaction {
        identity: Identity {
            scope: "private-draft".into(),
            model: "selected-model-and-encoding".into(),
            pinned: "trusted-state".into(),
        },
        expected_generation: 0,
        prior_digest: String::new(),
        prior_summary: None,
        groups: vec![Group {
            sequence: 1,
            text: "private customer source must never reach traces".into(),
        }],
        output_tokens: 2_048,
    }
}

#[test]
fn committed_summary_has_one_paired_trace_with_only_operation_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    let task = task();
    let capture = Capture::install();
    let mut trace = Trace::start("dispatch-one".into(), Host::Authoring, &task);
    trace.completed(1);
    trace.failed(error_code::INTERNAL_OPERATION_FAILED);
    drop(trace);
    assert_eq!(capture.events().len(), 2);
    assert_eq!(
        capture
            .only("context_compaction_started")
            .field("correlation_id"),
        Some("dispatch-one")
    );
    let completed = capture.only("context_compaction_completed");
    assert_eq!(completed.field("generation"), Some("1"));
    let _duration = completed
        .field("duration_ms")
        .ok_or("completion must record duration")?
        .parse::<u64>()?;
    assert_span_metadata(&capture, &task)
}

fn assert_span_metadata(
    capture: &Capture,
    task: &Compaction,
) -> Result<(), Box<dyn std::error::Error>> {
    let spans = capture.spans();
    assert_eq!(spans.len(), 1);
    let span = spans.first().ok_or("missing compaction span")?;
    assert_eq!(span.name, "context.compact");
    assert_eq!(span.field("host"), Some("authoring"));
    assert_eq!(span.field("expected_generation"), Some("0"));
    assert_eq!(span.field("generation"), Some("1"));
    assert_eq!(
        span.field("group_count"),
        Some(task.groups.len().to_string().as_str())
    );
    assert_eq!(
        span.field("reserved_output_tokens"),
        Some(task.output_tokens.to_string().as_str())
    );
    let recorded = format!("{spans:?}{:?}", capture.events());
    for private in [
        "private customer source",
        "private-draft",
        "trusted-state",
        "selected-model-and-encoding",
    ] {
        assert!(!recorded.contains(private), "private input reached tracing");
    }
    Ok(())
}

#[test]
fn refused_summary_records_one_failure_without_a_false_commit() {
    let task = task();
    let capture = Capture::install();
    let mut trace = Trace::start("lease-one".into(), Host::Runtime, &task);
    trace.failed(error_code::INTERNAL_OPERATION_FAILED);
    trace.completed(1);
    drop(trace);
    assert_eq!(capture.events().len(), 2);
    let failed = capture.only("context_compaction_failed");
    assert_eq!(
        failed.field("error_code"),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert_eq!(failed.field("reason"), Some("operation_failed"));
    assert_eq!(
        capture.spans().first().and_then(|span| span.field("host")),
        Some("runtime")
    );
    assert_eq!(
        capture
            .spans()
            .first()
            .and_then(|span| span.field("generation")),
        None
    );
}

#[test]
fn dropping_an_unfinished_summary_closes_the_trace_without_claiming_success() {
    let task = task();
    let capture = Capture::install();
    drop(Trace::start(
        "aborted-dispatch".into(),
        Host::Authoring,
        &task,
    ));
    assert_eq!(capture.events().len(), 2);
    let failed = capture.only("context_compaction_failed");
    assert_eq!(failed.field("reason"), Some("cancelled_or_dropped"));
    assert_eq!(failed.field("correlation_id"), Some("aborted-dispatch"));
    assert_eq!(
        capture
            .spans()
            .first()
            .and_then(|span| span.field("generation")),
        None
    );
}
