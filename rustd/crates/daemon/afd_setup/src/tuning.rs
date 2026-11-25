//! A fleet create's deployment settings: `[setup]` in the daemon's file.

use std::time::Duration;

/// How long a receipt answers its key when the deployment names nothing else.
///
/// Measured from the receipt's `created_at`: inside it the same key and body
/// replay the first result; at and past it the key is new and a claim of it
/// deletes the old receipt.
pub const DEFAULT_REPLAY_WINDOW: Duration = af_core::timing::DAY;

/// A fleet create's deployment settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupTuning {
    /// A repeated create within this returns the first answer.
    pub replay_window: Duration,
}

impl Default for SetupTuning {
    fn default() -> Self {
        Self {
            replay_window: DEFAULT_REPLAY_WINDOW,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SetupTuning;

    /// `[setup]` in the deployment file reaches the creates that enforce it.
    #[tokio::test]
    async fn configured_setup_settings_reach_the_creates() {
        let tuning = SetupTuning {
            replay_window: std::time::Duration::from_secs(60),
        };
        let page = af_validate::Ceiling::new(7, 3);
        let creations = crate::Creations::new(
            afd_db::test_util::unreachable_db(),
            af_crypto::entropy::Entropy::new(),
            afd_turns::Turns::for_tests(
                afd_db::test_util::unreachable_db(),
                afd_bus::test_util::unreachable(),
            ),
        )
        .with_tuning(tuning)
        .with_list_page(page);
        assert_eq!(creations.tuning, tuning);
        assert_eq!(creations.list_page, page);
    }
}
