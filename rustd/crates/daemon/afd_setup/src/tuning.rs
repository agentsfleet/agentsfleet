//! The setup chat's deployment settings: `[setup]` in the daemon's file.

use std::time::Duration;

/// How long one setup request may take before it is answered as busy, when
/// the deployment names nothing else.
pub const DEFAULT_BUDGET: Duration = Duration::from_secs(15);

/// How long a receipt answers its key when the deployment names nothing else.
///
/// Measured from the receipt's `created_at`: inside it the same key and body
/// replay the first result; at and past it the key is new and a claim of it
/// deletes the old receipt.
pub const DEFAULT_REPLAY_WINDOW: Duration = af_core::timing::DAY;

/// Drafts read per page by setup maintenance, when the deployment names
/// nothing else.
pub const DEFAULT_SCAN_BATCH: i64 = 64;

/// The setup chat's deployment settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupTuning {
    /// How long one setup request may take before it is answered as busy.
    pub budget: Duration,
    /// A repeated setup request within this returns the first answer.
    pub replay_window: Duration,
    /// Drafts read per page by setup maintenance.
    pub scan_batch: i64,
}

impl Default for SetupTuning {
    fn default() -> Self {
        Self {
            budget: DEFAULT_BUDGET,
            replay_window: DEFAULT_REPLAY_WINDOW,
            scan_batch: DEFAULT_SCAN_BATCH,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SetupTuning;

    /// `[setup]` in the deployment file reaches the drafts that enforce it.
    #[tokio::test]
    async fn configured_setup_settings_reach_the_drafts() {
        let tuning = SetupTuning {
            budget: std::time::Duration::from_secs(2),
            replay_window: std::time::Duration::from_secs(60),
            scan_batch: 9,
        };
        let page = af_validate::Ceiling::new(7, 3);
        let drafts = crate::Drafts::new(
            afd_db::test_util::unreachable_db(),
            af_crypto::entropy::Entropy::new(),
        )
        .with_tuning(tuning)
        .with_list_page(page);
        assert_eq!(drafts.tuning, tuning);
        assert_eq!(drafts.list_page, page);
        let defaults = crate::Drafts::new(
            afd_db::test_util::unreachable_db(),
            af_crypto::entropy::Entropy::new(),
        );
        assert_eq!(defaults.tuning, SetupTuning::default());
        assert_eq!(defaults.list_page, af_core::paging::CEILING);
    }
}
