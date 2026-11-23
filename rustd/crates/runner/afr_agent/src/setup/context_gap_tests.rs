//! A setup compaction over a history that does not reach back to the
//! watermark: the daemon sends every answered turn whatever the watermark,
//! and only the frame drops the oldest, so a turn older than the run's oldest
//! reaches no later run, and compaction goes on.

use std::error::Error;

use super::context::Saved;
use super::context_tests::checkpoint;
use super::fixture::{Context, setup_payload};
use af_secrets::Scrub;

type TestResult = Result<(), Box<dyn Error>>;

/// `count` earlier turns, each `text` asked and answered, numbered from
/// `first`.
fn turns(text: &str, first: u64, count: usize) -> Vec<(&str, &str, u64)> {
    (first..)
        .take(count)
        .map(|sequence| (text, text, sequence))
        .collect()
}

/// The watermark the compaction a run over `history` would make claims,
/// with a saved summary covering through `covered`; none when it makes
/// none.
async fn claimed(
    history: &[(&str, &str, u64)],
    covered: Option<u64>,
) -> Result<Option<u64>, Box<dyn Error>> {
    let run = setup_payload(history, 0, None);
    let stored = covered.map(|through| checkpoint(&run, None, through));
    let context = Context::stored(4, stored);
    let scrub = Scrub::new(&run.policy)?;
    let saved = Saved::load(&context, &run, &scrub).await;
    let task = saved.compaction(100, |_task| Ok(true))?;
    Ok(task.map(|task| task.covered_through()))
}

#[tokio::test]
async fn a_whole_history_compacts_its_oldest_turns() -> TestResult {
    assert_eq!(claimed(&turns("asked", 1, 5), None).await?, Some(3));
    Ok(())
}

/// Turns 3 and 4 fell outside the history the daemon sent, and no later run
/// is sent them either: the compaction of 5 to 7 is still made, or a draft
/// whose summary fell behind would never compact again.
#[tokio::test]
async fn a_history_past_the_watermark_still_compacts() -> TestResult {
    assert_eq!(claimed(&turns("asked", 5, 5), Some(2)).await?, Some(7));
    Ok(())
}

#[tokio::test]
async fn a_history_that_reaches_the_watermark_compacts_past_it() -> TestResult {
    assert_eq!(claimed(&turns("asked", 5, 5), Some(5)).await?, Some(7));
    Ok(())
}

/// With no summary saved, a history the frame cut compacts its oldest turns,
/// as the first save of a long draft must.
#[tokio::test]
async fn a_history_the_frame_cut_compacts_with_no_summary() -> TestResult {
    const SENT: usize = 8;
    let counted = turns("asked", 3, SENT);
    // Turns 3 on, all but the newest two.
    let through = 3 + u64::try_from(SENT)? - 2 - 1;
    assert_eq!(claimed(&counted, None).await?, Some(through));
    Ok(())
}
