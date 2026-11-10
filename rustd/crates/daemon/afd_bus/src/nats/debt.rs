//! Which subjects may have lost frames while this process published them.
//!
//! Core NATS acknowledges nothing. A frame written just before the socket died
//! may be gone, and a publish that failed certainly is. Either way the
//! subscribers are owed a gap marker on that subject once the client is back.
//! Both sets are bounded; past the bound, every subscriber everywhere is owed
//! one, through the shared broadcast subject.

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

/// What a reconnect must pay.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Owed {
    /// Each subject gets one marker.
    pub(super) subjects: Vec<String>,
    /// The sets overflowed; every hub gaps every subscription instead.
    pub(super) everyone: bool,
}

/// Recent publishes and unpaid markers, under one short lock.
#[derive(Debug)]
pub(super) struct Debt {
    /// Subjects published this recently before a disconnect are owed a marker.
    window: Duration,
    /// Subjects either set may name before it overflows into "everyone".
    max: usize,
    recent: HashMap<String, Instant>,
    /// When `recent` last refused a subject for lack of room.
    recent_overflow: Option<Instant>,
    /// When the oldest entry of a full `recent` leaves the window. Until then
    /// a prune frees nothing, so a full set is not rescanned on every publish.
    recent_frees_at: Option<Instant>,
    owed: HashSet<String>,
    everyone: bool,
}

impl Debt {
    /// Empty debt with the configured window and bound.
    pub(super) fn new(window: Duration, max: usize) -> Self {
        Self {
            window,
            max,
            recent: HashMap::new(),
            recent_overflow: None,
            recent_frees_at: None,
            owed: HashSet::new(),
            everyone: false,
        }
    }

    /// Remember a publish; a disconnect within the window owes it.
    pub(super) fn published(&mut self, subject: &str, now: Instant) {
        if let Some(at) = self.recent.get_mut(subject) {
            *at = now;
            return;
        }
        if self.recent.len() >= self.max && self.recent_frees_at.is_none_or(|at| now >= at) {
            let window = self.window;
            self.recent
                .retain(|_, at| now.saturating_duration_since(*at) <= window);
            self.recent_frees_at = self
                .recent
                .values()
                .min()
                .and_then(|oldest| oldest.checked_add(window))
                .and_then(|edge| edge.checked_add(Duration::from_nanos(1)));
        }
        if self.recent.len() >= self.max {
            self.recent_overflow = Some(now);
            return;
        }
        self.recent.insert(subject.to_owned(), now);
    }

    /// A publish that failed is owed, whatever the connection does next.
    pub(super) fn failed(&mut self, subject: &str) {
        self.owe(subject);
    }

    /// The socket died: every subject published inside the window is owed.
    pub(super) fn disconnected(&mut self, now: Instant) {
        let window = self.window;
        let fresh = |at: &Instant| now.saturating_duration_since(*at) <= window;
        if self.recent_overflow.as_ref().is_some_and(fresh) {
            self.everyone = true;
        }
        let owed: Vec<String> = self
            .recent
            .iter()
            .filter(|(_, at)| fresh(at))
            .map(|(subject, _)| subject.clone())
            .collect();
        for subject in owed {
            self.owe(&subject);
        }
        self.recent.clear();
        self.recent_overflow = None;
        self.recent_frees_at = None;
    }

    /// Hand the debt to the payer; anything it cannot pay comes back through
    /// [`Self::failed`] or [`Self::everyone`].
    pub(super) fn take(&mut self) -> Owed {
        let mut subjects: Vec<String> = self.owed.drain().collect();
        subjects.sort_unstable();
        Owed {
            subjects,
            everyone: std::mem::take(&mut self.everyone),
        }
    }

    /// Whether nothing is owed: every named marker and the broadcast are paid.
    pub(super) fn is_settled(&self) -> bool {
        self.owed.is_empty() && !self.everyone
    }

    /// Owe every subscriber a gap, as when the broadcast marker failed.
    pub(super) const fn everyone(&mut self) {
        self.everyone = true;
    }

    fn owe(&mut self, subject: &str) {
        if self.everyone {
            return;
        }
        if self.owed.len() >= self.max && !self.owed.contains(subject) {
            // One broadcast marker replaces every named one.
            self.owed.clear();
            self.everyone = true;
            return;
        }
        self.owed.insert(subject.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{Debt, Owed};

    const RECENT_WINDOW: Duration = Duration::from_secs(2);
    const DEBT_SUBJECTS_MAX: usize = 64;

    fn debt() -> Debt {
        Debt::new(RECENT_WINDOW, DEBT_SUBJECTS_MAX)
    }

    #[test]
    fn a_failed_publish_is_owed_once() {
        let mut debt = debt();
        debt.failed("af.fleet.a.tail");
        debt.failed("af.fleet.a.tail");
        assert_eq!(debt.take().subjects, ["af.fleet.a.tail"]);
        assert_eq!(debt.take(), Owed::default(), "paid debt is gone");
    }

    #[test]
    fn a_disconnect_owes_only_the_subjects_inside_the_window() {
        let start = Instant::now();
        let mut debt = debt();
        debt.published("af.fleet.old.tail", start);
        debt.published("af.fleet.new.tail", start + Duration::from_secs(3));
        debt.disconnected(start + Duration::from_secs(3) + RECENT_WINDOW);
        assert_eq!(debt.take().subjects, ["af.fleet.new.tail"]);
    }

    #[test]
    fn a_republish_refreshes_its_time() {
        let start = Instant::now();
        let mut debt = debt();
        debt.published("af.wake.3", start);
        debt.published("af.wake.3", start + Duration::from_secs(5));
        debt.disconnected(start + Duration::from_secs(6));
        assert_eq!(debt.take().subjects, ["af.wake.3"]);
    }

    #[test]
    fn a_disconnect_forgets_what_it_counted() {
        let now = Instant::now();
        let mut debt = debt();
        debt.published("af.fleet.a.tail", now);
        debt.disconnected(now);
        let _ = debt.take();
        debt.disconnected(now);
        assert_eq!(
            debt.take(),
            Owed::default(),
            "a second drop owes nothing new"
        );
    }

    #[test]
    fn too_many_owed_subjects_owe_everyone_instead() {
        let mut debt = debt();
        for index in 0..DEBT_SUBJECTS_MAX {
            debt.failed(&format!("af.fleet.{index}.tail"));
        }
        debt.failed("af.fleet.0.tail");
        assert!(!debt.everyone, "a known subject fits");
        debt.failed("af.fleet.overflow.tail");
        debt.failed("af.fleet.after.tail");
        let owed = debt.take();
        assert!(owed.everyone);
        assert_eq!(
            owed.subjects,
            Vec::<String>::new(),
            "one broadcast replaces the names"
        );
        assert_eq!(debt.take(), Owed::default());
    }

    #[test]
    fn a_full_recent_set_owes_everyone_on_disconnect() {
        let now = Instant::now();
        let mut debt = debt();
        for index in 0..=DEBT_SUBJECTS_MAX {
            debt.published(&format!("af.fleet.{index}.tail"), now);
        }
        debt.disconnected(now);
        assert!(debt.take().everyone);
    }

    #[test]
    fn stale_recent_subjects_make_room_before_overflow() {
        let start = Instant::now();
        let later = start + RECENT_WINDOW + Duration::from_millis(1);
        let mut debt = debt();
        for index in 0..DEBT_SUBJECTS_MAX {
            debt.published(&format!("af.fleet.{index}.tail"), start);
        }
        debt.published("af.fleet.fresh.tail", later);
        debt.disconnected(later);
        let owed = debt.take();
        assert!(!owed.everyone, "stale entries were evicted, not overflowed");
        assert_eq!(owed.subjects, ["af.fleet.fresh.tail"]);
    }

    #[test]
    fn a_full_set_makes_room_once_its_oldest_leaves_the_window() {
        let start = Instant::now();
        let mut debt = debt();
        for index in 0..DEBT_SUBJECTS_MAX {
            debt.published(&format!("af.fleet.{index}.tail"), start);
        }
        let inside = start + RECENT_WINDOW / 2;
        debt.published("af.fleet.early.tail", inside);
        assert_eq!(
            debt.recent_overflow,
            Some(inside),
            "nothing could leave yet"
        );
        assert!(
            debt.recent_frees_at
                .is_some_and(|at| at > start + RECENT_WINDOW)
        );

        let past = start + RECENT_WINDOW + Duration::from_millis(1);
        debt.published("af.fleet.late.tail", past);
        assert!(
            debt.recent.contains_key("af.fleet.late.tail"),
            "the stale were pruned"
        );
        assert!(!debt.recent.contains_key("af.fleet.early.tail"));
        debt.disconnected(past);
        assert!(
            debt.take().everyone,
            "the refusal inside the window still owes everyone"
        );
    }

    #[test]
    fn an_old_overflow_owes_nobody() {
        let start = Instant::now();
        let mut debt = debt();
        for index in 0..=DEBT_SUBJECTS_MAX {
            debt.published(&format!("af.fleet.{index}.tail"), start);
        }
        debt.disconnected(start + RECENT_WINDOW + Duration::from_secs(1));
        assert_eq!(debt.take(), Owed::default());
    }

    #[test]
    fn a_failed_broadcast_owes_everyone_again() {
        let mut debt = debt();
        debt.everyone();
        debt.failed("af.fleet.a.tail");
        let owed = debt.take();
        assert!(owed.everyone);
        assert_eq!(owed.subjects, Vec::<String>::new());
    }
}
