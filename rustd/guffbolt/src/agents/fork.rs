//! A child forked from its parent: the parent's turns, which the child
//! starts from before its own task.

use super::Fork;
use crate::transcript::Message;

/// What `fork` keeps of `history`, turn by turn, each turn from a user
/// message to the next: nothing for [`Fork::None`]; everything from the
/// first user message for [`Fork::All`], or for a [`Fork::Last`] longer
/// than `history`; otherwise the last that many turns, and nothing before
/// them, so what a child reads of its parent is exactly what was asked for.
pub(super) fn forked(history: &[Message], fork: Fork) -> Vec<Message> {
    let turns = match fork {
        Fork::None => return Vec::new(),
        Fork::All => usize::MAX,
        Fork::Last(turns) => turns.get(),
    };
    let starts: Vec<usize> = history
        .iter()
        .enumerate()
        .filter(|(_, message)| matches!(message, Message::User(_)))
        .map(|(at, _)| at)
        .collect();
    let first = starts
        .len()
        .checked_sub(turns)
        .and_then(|from| starts.get(from))
        .or_else(|| starts.first());
    first.map_or_else(Vec::new, |&from| {
        history.get(from..).unwrap_or_default().to_vec()
    })
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::forked;
    use crate::agents::Fork;
    use crate::transcript::{Message, Replay};

    fn user(text: &str) -> Message {
        Message::User(text.to_owned())
    }

    fn said(text: &str) -> Message {
        Message::Assistant {
            text: text.to_owned(),
            calls: Vec::new(),
            replay: Replay::default(),
        }
    }

    fn last(turns: usize) -> Fork {
        NonZeroUsize::new(turns).map_or(Fork::None, Fork::Last)
    }

    #[test]
    fn a_fork_keeps_the_last_turns_and_none_before() {
        let history = [
            user("one"),
            said("1"),
            user("two"),
            said("2"),
            user("three"),
        ];

        assert_eq!(forked(&history, Fork::None), Vec::<Message>::new());
        assert_eq!(forked(&history, Fork::All), history.to_vec());
        assert_eq!(forked(&history, last(1)), [user("three")]);
        assert_eq!(
            forked(&history, last(2)),
            [user("two"), said("2"), user("three")]
        );
        assert_eq!(
            forked(&history, last(9)),
            history.to_vec(),
            "every turn there is"
        );
        assert_eq!(forked(&[said("alone")], Fork::All), Vec::<Message>::new());
    }

    /// `fork_turns` reads as `all` when blank, `none` and
    /// `all` in any case, or a number of turns from 1.
    #[test]
    fn fork_turns_reads_none_all_or_a_number_of_turns() {
        assert_eq!(Fork::parse("none"), Some(Fork::None));
        assert_eq!(Fork::parse(" NONE "), Some(Fork::None));
        assert_eq!(Fork::parse("all"), Some(Fork::All));
        assert_eq!(Fork::parse("All"), Some(Fork::All));
        assert_eq!(Fork::parse(""), Some(Fork::All));
        assert_eq!(Fork::parse("3"), Some(last(3)));
        for refused in ["0", "-1", "some", "2.5", "last 2"] {
            assert_eq!(Fork::parse(refused), None, "{refused}");
        }
    }
}
