//! Keep a report within the wire frame when its last answer or memory grew
//! after the calls emitted their live completions. The full items already
//! posted under this fence hold the identical sources for durable history.
use std::borrow::Cow;
use std::num::NonZeroU64;

use af_wire::protocol::{MESSAGE_BYTES_MAX, Up};
use af_wire::report::ReportRequest;
use af_wire::tool_trace::ToolTrace;
use zeroize::Zeroizing;

use crate::{Result, error};

pub(crate) fn encode(request: &ReportRequest<'_>) -> Result<Zeroizing<Vec<u8>>> {
    let encode = |report: &ReportRequest<'_>| {
        serde_json::to_vec(report)
            .map(Zeroizing::new)
            .map_err(error::encode)
    };
    let Some(raw) = request.tool_calls else {
        return encode(request);
    };
    // The request id is assigned later; its longest encoding bounds every send.
    let envelope = Up::Report {
        request_id: NonZeroU64::MAX,
        request: Box::new(Cow::Borrowed(request)),
    };
    if af_validate::serialized_within(&envelope, MESSAGE_BYTES_MAX) {
        return encode(request);
    }
    let Ok(mut trace) = raw.narrow() else {
        return encode(request);
    };
    if !take_sources(&mut trace) {
        return encode(request);
    }
    let packed = serde_json::to_string(&trace)
        .map(Zeroizing::new)
        .map_err(error::encode)?;
    let mut compact = request.clone();
    compact.tool_calls = Some(serde_json::from_str(&packed).map_err(error::encode)?);
    encode(&compact)
}

fn take_sources(trace: &mut ToolTrace<'_>) -> bool {
    if !trace.calls.iter().any(|call| !call.sources.is_empty()) {
        return false;
    }
    for call in &mut trace.calls {
        call.sources.clear();
    }
    trace.sources_in_items = true;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn expanded_report_cites_its_fenced_items_when_the_final_answer_fills_the_frame()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let raw = crate::test_support::report_body();
        let mut report: Value = serde_json::from_slice(&raw)?;
        let source =
            json!({"url":"https://docs.example.test/","title":"a".repeat(MESSAGE_BYTES_MAX / 2)});
        report["tool_calls"] = json!({"calls":[{
            "call_id":"1", "name":"web_search", "arguments":{}, "status":"succeeded",
            "duration_ms":1, "sources":[source]
        }]});
        report["response_text"] = Value::String("b".repeat(MESSAGE_BYTES_MAX / 2));
        let raw = serde_json::to_vec(&report)?;
        let wire: ReportRequest<'_> = serde_json::from_slice(&raw)?;
        let saved = encode(&wire)?;
        let compact: ReportRequest<'_> = serde_json::from_slice(&saved)?;
        assert_eq!(wire.response_text, compact.response_text);
        let trace = compact
            .tool_calls
            .ok_or("missing trace")?
            .narrow()
            .map_err(|cause| cause.as_str())?;
        assert!(trace.sources_in_items);
        assert!(trace.calls.iter().all(|call| call.sources.is_empty()));
        let frame = Up::Report {
            request_id: NonZeroU64::MAX,
            request: Box::new(Cow::Borrowed(&compact)),
        };
        assert!(af_validate::serialized_within(&frame, MESSAGE_BYTES_MAX));
        Ok(())
    }
}
