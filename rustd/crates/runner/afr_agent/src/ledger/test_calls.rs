//! The calls the ledger's cases run through it, opened and ended as
//! `guffbolt` opens and ends them.

use std::time::Duration;

use afr_tools::catalog::UPDATE_PLAN;
use guffbolt::{Call, CallEnd, CallRef, Mark, ToolOutput};
use tracing::Span;

use super::{Ledger, Opened};

/// The run every call here belongs to.
pub(super) const RUN_ID: &str = "run-1";
/// The turn the run was assigned for.
pub(super) const TURN: &str = "019f0000-0000-7000-8000-000000000001";

/// Ends `call`, which `opened` holds, as `guffbolt` ends it: answering
/// `output`, or interrupted when there is none. Hands back what the model
/// reads.
pub(super) fn close(
    ledger: &Ledger<'_>,
    call: &Call,
    opened: (Opened, Span),
    output: Option<ToolOutput>,
) -> Option<ToolOutput> {
    marked(ledger, call, opened, output, None)
}

/// [`close`], for a call whose step began with `mark` the newest input's.
pub(super) fn marked(
    ledger: &Ledger<'_>,
    call: &Call,
    (opened, span): (Opened, Span),
    mut output: Option<ToolOutput>,
    mark: Option<&Mark>,
) -> Option<ToolOutput> {
    let at = CallRef {
        step: 1,
        index: 0,
        call_id: call.id.clone(),
    };
    let end = CallEnd {
        call,
        at: &at,
        mark,
        span: &span,
        duration: Duration::ZERO,
        output: output.as_mut(),
    };
    ledger.close(opened, end);
    output
}

/// Opens `call` and ends it at once, answering `output`, or interrupted
/// when there is none.
pub(super) fn ran(
    ledger: &Ledger<'_>,
    call: &Call,
    output: Option<ToolOutput>,
) -> Option<ToolOutput> {
    close(ledger, call, ledger.open(call), output)
}

/// Runs one call through `ledger`, answering `text`.
pub(super) fn call(ledger: &Ledger<'_>, text: String) {
    ran(ledger, &planned(), Some(ToolOutput::succeeded(text)));
}

/// A call to `UPDATE_PLAN`, which every call here stands in for.
pub(super) fn planned() -> Call {
    Call {
        id: String::new(),
        name: UPDATE_PLAN.name().to_owned(),
        arguments: serde_json::json!({}),
    }
}
