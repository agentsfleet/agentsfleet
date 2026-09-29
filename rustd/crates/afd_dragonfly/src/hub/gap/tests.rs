//! The attribution rules, driven with explicit instants rather than sockets.

use std::time::Duration;

use tokio::time::Instant;

use super::{Attribution, Burst, Cause, GAP_LOG_BURST, Loss, NODE_REPAIR_WINDOW};

const CHANNEL: &str = "fleet:a:activity";
const OTHER: &str = "fleet:b:activity";

/// A node's loss explained by a confirmation: nothing redials, and the
/// confirmation is labelled a repair.
#[test]
fn a_replay_inside_the_window_explains_the_loss() {
    let now = Instant::now();
    let mut attribution = Attribution::new(Vec::new());
    assert_eq!(attribution.disconnected(now, &[]), None);
    assert_eq!(attribution.deadline(), Some(now + NODE_REPAIR_WINDOW));

    assert_eq!(attribution.replayed(CHANNEL), Cause::NodeRepaired);
    assert_eq!(attribution.deadline(), None, "the loss is explained");
    assert_eq!(attribution.expired(now + NODE_REPAIR_WINDOW), None);
    // The node's other channels replay after the first explained it.
    assert_eq!(attribution.replayed(OTHER), Cause::Resubscribed);
}

/// A window that closes with no replay is a redial, once.
#[test]
fn an_unexplained_loss_redials_when_its_window_closes() {
    let now = Instant::now();
    let mut attribution = Attribution::new(Vec::new());
    assert_eq!(attribution.disconnected(now, &[]), None);
    let inside = now + NODE_REPAIR_WINDOW - Duration::from_millis(1);
    assert_eq!(attribution.expired(inside), None, "still waiting");
    assert_eq!(
        attribution.expired(now + NODE_REPAIR_WINDOW),
        Some(Loss::Unexplained)
    );
    assert_eq!(attribution.expired(now + NODE_REPAIR_WINDOW * 2), None);
}

/// Two nodes lost at once cannot be told apart by their replays.
#[test]
fn a_second_loss_while_the_first_is_pending_redials_at_once() {
    let now = Instant::now();
    let mut attribution = Attribution::new(Vec::new());
    assert_eq!(attribution.disconnected(now, &[]), None);
    assert_eq!(
        attribution.disconnected(now + Duration::from_millis(3), &[]),
        Some(Loss::Simultaneous)
    );
}

/// After one loss is explained, the next is its own.
#[test]
fn an_explained_loss_leaves_the_next_one_its_own_window() {
    let now = Instant::now();
    let mut attribution = Attribution::new(Vec::new());
    assert_eq!(attribution.disconnected(now, &[]), None);
    assert_eq!(attribution.replayed(CHANNEL), Cause::NodeRepaired);
    assert_eq!(attribution.disconnected(now, &[]), None);
}

/// A slot move and a redial name themselves, once per channel, ahead of a
/// pending loss.
#[test]
fn moved_and_redialled_channels_name_their_own_cause() {
    let now = Instant::now();
    let mut attribution = Attribution::new(vec![OTHER.to_owned()]);
    attribution.moved(CHANNEL.to_owned());
    assert_eq!(attribution.disconnected(now, &[]), None);

    assert_eq!(attribution.replayed(CHANNEL), Cause::SlotMoved);
    assert_eq!(attribution.replayed(OTHER), Cause::Reconnected);
    assert_eq!(
        attribution.deadline(),
        Some(now + NODE_REPAIR_WINDOW),
        "neither replay was the lost node's"
    );
    assert_eq!(attribution.replayed(CHANNEL), Cause::NodeRepaired);
}

/// Fifty gapped channels in one burst are one warning per cause.
#[test]
fn a_burst_gathers_every_gap_until_it_is_due() {
    let now = Instant::now();
    let mut burst = Burst::default();
    assert_eq!(burst.due(), None);
    for offset in 0..50 {
        burst.record(Cause::NodeRepaired, now + Duration::from_millis(offset));
    }
    burst.record(Cause::SlotMoved, now);
    assert_eq!(
        burst.due(),
        Some(now + GAP_LOG_BURST),
        "opened by the first"
    );

    let tally = burst.take();
    assert_eq!(tally.get(&Cause::NodeRepaired), Some(&50));
    assert_eq!(tally.get(&Cause::SlotMoved), Some(&1));
    assert_eq!(burst.due(), None, "taking closes the burst");
    assert!(burst.take().is_empty());
}

/// Every label the log carries is the spec's spelling.
#[test]
fn causes_and_losses_render_their_log_names() {
    let names: Vec<_> = [
        Cause::NodeRepaired,
        Cause::SlotMoved,
        Cause::Reconnected,
        Cause::Resubscribed,
    ]
    .into_iter()
    .map(Cause::as_str)
    .collect();
    assert_eq!(
        names,
        ["node_repaired", "slot_moved", "reconnected", "resubscribed"]
    );
    let losses: Vec<_> = [
        Loss::Unexplained,
        Loss::Simultaneous,
        Loss::Closed,
        Loss::CommandFailed,
    ]
    .into_iter()
    .map(Loss::as_str)
    .collect();
    assert_eq!(
        losses,
        ["unexplained", "simultaneous", "closed", "command_failed"]
    );
}

/// Every channel the hub re-subscribed after a loss is labelled a repair,
/// not only the one whose confirmation explained it.
#[test]
fn every_channel_re_subscribed_after_a_loss_is_a_repair() {
    let now = Instant::now();
    let mut attribution = Attribution::new(Vec::new());
    let live = [CHANNEL.to_owned(), OTHER.to_owned()];
    assert_eq!(attribution.disconnected(now, &live), None);
    assert_eq!(attribution.replayed(OTHER), Cause::NodeRepaired);
    assert_eq!(
        attribution.deadline(),
        None,
        "the first confirmation explains it"
    );
    assert_eq!(attribution.replayed(CHANNEL), Cause::NodeRepaired);
    assert_eq!(
        attribution.replayed(CHANNEL),
        Cause::Resubscribed,
        "a channel is labelled once per loss"
    );
}
