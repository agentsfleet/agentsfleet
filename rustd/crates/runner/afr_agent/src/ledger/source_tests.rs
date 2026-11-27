//! One sanitized ordered list is shared by live completions and durable history.
#![expect(
    clippy::indexing_slicing,
    reason = "test assertions index known call fixtures"
)]
use af_wire::activity::ActivityFrame;
use af_wire::tool_trace::{WebSource, encoded_len};
use guffbolt::{Call, Source, ToolOutput};
use serde_json::json;

use super::Ledger;
use super::test_calls::{RUN_ID, TURN, planned, ran};
use crate::fixture::{Frames, scrub};

fn source(url: &str, title: &str) -> Source {
    Source {
        url: url.to_owned(),
        title: Some(title.to_owned()),
    }
}

#[test]
fn a_completed_search_carries_its_sources() {
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);
    let input = [
        source("https://one.test/a", "One"),
        source("https://two.test/b", "Two"),
    ];
    let call = Call {
        name: "web_search".into(),
        ..planned()
    };
    ran(
        &ledger,
        &call,
        Some(ToolOutput::succeeded("results").with_sources(input.to_vec())),
    );
    ran(
        &ledger,
        &call,
        Some(
            ToolOutput::failed(guffbolt::ErrorCode::new("network_restricted"), "Allow all")
                .with_sources(input.to_vec()),
        ),
    );
    let (trace, records) = ledger.finish();
    let end: Vec<_> = frames
        .taken()
        .into_iter()
        .filter_map(|f| match f {
            ActivityFrame::ToolCallCompleted(frame) => Some(frame.sources),
            _ => None,
        })
        .collect();
    let desired: Vec<_> = input
        .into_iter()
        .map(|s| WebSource {
            url: s.url,
            title: s.title,
        })
        .collect();
    assert_eq!(end, [desired.clone(), vec![]]);
    assert_eq!(trace.calls[0].sources, desired);
    assert_eq!(records[0].sources, desired);
    assert!(trace.calls[1].sources.is_empty() && records[1].sources.is_empty());
}

#[test]
fn signed_redirects_and_titles_are_absent_from_every_disclosure() {
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);
    let requested = "https://docs.test/page?signature=private";
    let redirected = "https://support.test/private-route?private-token";
    let call = Call {
        name: "web_fetch".into(),
        arguments: json!({"url":requested}),
        ..planned()
    };
    let output = ToolOutput::succeeded(format!("{requested} via {redirected}")).with_sources(vec![
        source("https://docs.test/page", requested),
        source(redirected, redirected),
    ]);
    ran(&ledger, &call, Some(output));
    let (trace, records) = ledger.finish();
    assert_eq!(
        json!(trace.calls[0].sources),
        json!([{"url":"https://docs.test/page"},{"url":"https://support.test"}])
    );
    assert_eq!(trace.calls[0].sources, records[0].sources);
    assert!(
        !json!([trace, records, frames.taken()])
            .to_string()
            .contains("private")
    );
}

#[test]
fn an_escaped_source_is_kept_whole_or_omitted() {
    let sample = WebSource {
        url: "https://safe.test/".into(),
        title: Some("a \"quoted\" title".into()),
    };
    let first = br#","sources":[]"#.len() + encoded_len(&sample);
    for (room, expected) in [
        (first - 1, 0),
        (first, 1),
        (first + encoded_len(&sample), 1),
        (first + 1 + encoded_len(&sample), 2),
    ] {
        let mut pages = vec![sample.clone(), sample.clone()];
        super::sources::fit(&mut pages, room);
        assert_eq!(pages, vec![sample.clone(); expected]);
    }
}

#[tokio::test]
async fn credentials_minted_after_the_ledger_opens_are_masked_in_web_activity_and_history()
-> Result<(), Box<dyn std::error::Error>> {
    use af_core::clock::FixedClock;
    use af_egress::fixture::{START, TOKEN_LIFETIME, policy};
    use af_egress::testing::CountingMint;
    use af_egress::{Draft, Egress, Placement};
    let clock = FixedClock::at(START);
    let token = "ephemeral-test-value";
    let mint = CountingMint::answering(token, TOKEN_LIFETIME, clock.clone());
    let policy = policy();
    let mut guard = Egress::new(RUN_ID, &policy, &mint, &clock);
    let minted = guard.minted_mask();
    let calls = afr_tools::CallNumbers::default();
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub).numbering(&calls, &minted);
    guard
        .prepare(Draft {
            method: "GET".into(),
            url: "https://api.github.com/repos/acme/widgets".into(),
            headers: vec![(
                "Authorization".into(),
                "Bearer ${secrets.github.token}".into(),
            )],
            body: None,
            placement: Placement::Authorization,
        })
        .await?;
    assert_eq!(mint.asked(), 1);
    let target = format!("https://docs.test/{token}");
    for call in [
        Call {
            name: "web_fetch".into(),
            arguments: json!({"url":target}),
            ..planned()
        },
        Call {
            name: "web_search".into(),
            arguments: json!({"query":token}),
            ..planned()
        },
    ] {
        let answer = ToolOutput::succeeded(token).with_sources(vec![source(&target, token)]);
        let returned = ran(&ledger, &call, Some(answer)).ok_or("no tool output")?;
        assert!(!returned.text.contains(token));
    }
    let recorded = json!([frames.taken(), ledger.finish()]).to_string();
    assert!(!recorded.contains(token));
    assert!(recorded.contains("secret:github.token"));
    Ok(())
}
