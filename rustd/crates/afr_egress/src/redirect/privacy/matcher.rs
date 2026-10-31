//! Collect the complete union of known, possibly overlapping private spellings.

use std::collections::HashMap;
use std::ops::Range;

use regex_automata::dfa::{Automaton, dense};
use regex_automata::nfa::thompson;
use regex_automata::util::primitives::StateID;
use regex_automata::{Input, MatchKind};

use super::{Stop, encoded_pattern};
use crate::Result;

const CONSTRUCTION_MAX_BYTES: usize = 10 * 1024 * 1024;

pub(super) struct Matcher {
    dfa: dense::DFA<Vec<u32>>,
    widths: Vec<usize>,
}

impl Matcher {
    pub(super) fn new(values: &[String]) -> Result<Self, Stop> {
        let patterns: Vec<_> = values.iter().map(|value| encoded_pattern(value)).collect();
        let dfa = dense::Builder::new()
            .configure(
                dense::Config::new()
                    .match_kind(MatchKind::All)
                    .dfa_size_limit(Some(CONSTRUCTION_MAX_BYTES))
                    .determinize_size_limit(Some(CONSTRUCTION_MAX_BYTES)),
            )
            .thompson(thompson::Config::new().nfa_size_limit(Some(CONSTRUCTION_MAX_BYTES)))
            .build_many(&patterns)
            .map_err(|_unavailable| Stop::Privacy)?;
        // Every spelling is nonempty; only fixed-width ASCII hex classes vary.
        Ok(Self {
            dfa,
            widths: values.iter().map(String::len).collect(),
        })
    }

    pub(super) fn covered(&self, text: &str) -> Result<Vec<Range<usize>>, Stop> {
        let mut ranges = Vec::new();
        let mut widths = HashMap::new();
        let mut state = self
            .dfa
            .start_state_forward(&Input::new(text))
            .map_err(|_unavailable| Stop::Privacy)?;
        for (end, &byte) in text.as_bytes().iter().enumerate() {
            state = self.dfa.next_state(state, byte);
            // DFA matches are delayed one byte: this state's exclusive end is `end`.
            self.record(state, end, &mut widths, &mut ranges)?;
        }
        state = self.dfa.next_eoi_state(state);
        self.record(state, text.len(), &mut widths, &mut ranges)?;
        Ok(ranges)
    }

    fn record(
        &self,
        state: StateID,
        end: usize,
        widths: &mut HashMap<StateID, usize>,
        ranges: &mut Vec<Range<usize>>,
    ) -> Result<(), Stop> {
        if !self.dfa.is_match_state(state) {
            return Ok(());
        }
        let width = if let Some(&width) = widths.get(&state) {
            width
        } else {
            let width = self.max_width(state)?;
            widths.insert(state, width);
            width
        };
        let start = end.checked_sub(width).ok_or(Stop::Privacy)?;
        let mut covered = start..end;
        // Ends arrive in order. A later, longer match may bridge earlier ranges.
        while let Some(previous) = ranges.pop_if(|previous| previous.end > covered.start) {
            covered.start = covered.start.min(previous.start);
        }
        ranges.push(covered);
        Ok(())
    }

    fn max_width(&self, state: StateID) -> Result<usize, Stop> {
        (0..self.dfa.match_len(state)).try_fold(0_usize, |width, index| {
            let pattern = self.dfa.match_pattern(state, index).as_usize();
            self.widths
                .get(pattern)
                .map(|candidate| width.max(*candidate))
                .ok_or(Stop::Privacy)
        })
    }
}
