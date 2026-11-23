//! The edges of a child's turn: an interrupt or a close before the turn
//! begins, a follow-up its turn never read, the run's end over idle
//! children, and the steps a dropped turn began.

use super::*;

/// A turn interrupted before it begins never runs: the child ends
/// interrupted and stays open, and a follow-up runs it again.
#[test]
fn an_interrupt_before_the_turn_begins_ends_it_unrun() {
    let (agents, _requests) = Agents::new(CAPS);
    assert_eq!(reserve(&agents, Me::ROOT, "work"), Ok(1));

    assert_eq!(agents.interrupt(1), Some(Kind::Running));
    assert!(agents.begin(1).is_none(), "the interrupted turn never runs");
    assert_eq!(kinds(&agents), [Kind::Interrupted]);
    assert_eq!(agents.follow_up(1, "again".to_owned()), Some(true));
    assert!(agents.begin(1).is_some(), "the follow-up's turn runs");
}

/// A child closed before its turn begins ends closed with the run, never
/// interrupted, so a run saved after it does not bring it back open.
#[test]
fn a_close_before_the_turn_begins_stays_closed_through_a_save() {
    let (agents, _requests) = Agents::new(CAPS);
    assert_eq!(reserve(&agents, Me::ROOT, "work"), Ok(1));

    assert_eq!(agents.close(1), Some(Kind::Running));
    agents.end_all();
    let pause = agents.try_pause(|| true);
    let mut state = DriveState::default();
    let saved = pause
        .as_ref()
        .is_some_and(|pause| agents.save(pause, &mut state));
    let (restored, _requests) = Agents::restored(CAPS, &state);

    assert!(saved);
    assert_eq!(kinds(&agents), [Kind::Closed]);
    assert_eq!(kinds(&restored), [Kind::Closed], "it comes back closed");
    assert_eq!(
        reserve(&restored, Me::ROOT, "next"),
        Ok(2),
        "its seat is free"
    );
}

/// The run's end interrupts only a running child: an idle one keeps the
/// answer or the failure its last turn left, so a resumed run reads it.
#[test]
fn the_runs_end_keeps_what_an_idle_child_finished_with() {
    let (agents, _requests) = Agents::new(Caps { open: 3, ..CAPS });
    assert_eq!(reserve(&agents, Me::ROOT, "a"), Ok(1));
    assert_eq!(reserve(&agents, Me::ROOT, "b"), Ok(2));
    assert_eq!(reserve(&agents, Me::ROOT, "c"), Ok(3));
    answer(&agents, 1, "a done", Vec::new());
    assert!(agents.begin(2).is_some());
    agents.ended(2, Status::Failed("b broke".to_owned()), None);

    agents.end_all();

    assert_eq!(
        kinds(&agents),
        [Kind::Done, Kind::Failed, Kind::Interrupted]
    );
}

/// A follow-up taken while a turn runs, which that turn ends without
/// reading, runs a turn of its own; one the turn read is owed none.
#[test]
fn a_follow_up_the_turn_never_read_runs_a_turn_of_its_own() {
    let (agents, _requests) = Agents::new(CAPS);
    assert_eq!(reserve(&agents, Me::ROOT, "work"), Ok(1));
    assert!(agents.begin(1).is_some());

    assert_eq!(agents.interrupt(1), Some(Kind::Running));
    assert_eq!(agents.follow_up(1, "go on".to_owned()), Some(true));
    agents.ended(1, Status::Interrupted, None);

    assert_eq!(kinds(&agents), [Kind::Running], "the child runs again");
    assert!(agents.begin(1).is_some(), "with a turn queued");
    assert_eq!(agents.take_inbox(1), ["work", "go on"]);
    assert_eq!(agents.follow_up(1, "and more".to_owned()), Some(true));
    assert_eq!(agents.take_inbox(1), ["and more"], "this turn read it");
    agents.ended(1, Status::Done("went on".to_owned()), None);
    assert_eq!(kinds(&agents), [Kind::Done], "no turn is owed");
}

/// A turn dropped before its end keeps nothing of its state, yet the steps
/// it began are counted, saved and restored, so the next turn numbers its
/// steps after them.
#[test]
fn a_dropped_turns_steps_number_the_next_turn_after_a_restore() {
    let (agents, _requests) = Agents::new(CAPS);
    assert_eq!(reserve(&agents, Me::ROOT, "work"), Ok(1));
    assert!(agents.begin(1).is_some());
    agents.lent(1, 3);
    agents.lent(9, 7);
    agents.ended(1, Status::Interrupted, None);

    let pause = agents.try_pause(|| true);
    let mut state = DriveState::default();
    let saved = pause
        .as_ref()
        .is_some_and(|pause| agents.save(pause, &mut state));
    drop(pause);
    let (restored, _requests) = Agents::restored(CAPS, &state);
    assert_eq!(restored.follow_up(1, "again".to_owned()), Some(true));
    let next = restored.begin(1).map(|turn| turn.state.completed_steps());

    assert!(saved);
    assert_eq!(
        state
            .children()
            .children
            .iter()
            .map(|child| (child.id, child.steps))
            .collect::<Vec<_>>(),
        [(1, 3)],
        "a step lent for an id no child has counts for none"
    );
    assert_eq!(next, Some(3), "the next turn's first step is the fourth");
}
