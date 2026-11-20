//! A setup compaction claims only the turns its run read: a history that
//! does not reach back to the saved summary's watermark, or one the daemon's
//! bounds cut with no summary saved, compacts nothing.

use std::error::Error;

use af_secrets::Scrub;
use af_wire::run::HISTORY_TURNS_MAX;

use super::context::Saved;
use super::context_tests::checkpoint;
use super::fixture::{Context, setup_payload};

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

/// Turns 3 and 4 fell outside the history the daemon sent: a compaction of
/// 5 to 7 would claim them through its watermark, so none is made.
#[tokio::test]
async fn a_history_short_of_the_watermark_compacts_nothing() -> TestResult {
    assert_eq!(claimed(&turns("asked", 5, 5), Some(2)).await?, None);
    Ok(())
}

/// The history holds the turn the summary covers through, so every turn
/// after it was read, and the compaction goes on from there.
#[tokio::test]
async fn a_history_that_reaches_the_watermark_compacts_past_it() -> TestResult {
    assert_eq!(claimed(&turns("asked", 5, 5), Some(5)).await?, Some(7));
    Ok(())
}

/// With no summary saved, a history the daemon's bounds may have cut is not
/// known to start at the draft's first turn.
#[tokio::test]
async fn a_history_the_bounds_may_have_cut_compacts_nothing() -> TestResult {
    let counted = turns("asked", 3, HISTORY_TURNS_MAX);
    let long = "x".repeat(4_000);
    let sized = turns(&long, 1, 5);

    assert_eq!(
        claimed(&counted, None).await?,
        None,
        "as many turns as sent at most"
    );
    assert_eq!(
        claimed(&sized, None).await?,
        None,
        "no room for one more turn"
    );
    Ok(())
}
