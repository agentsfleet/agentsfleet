use super::Recovery;
use std::{collections::HashSet, time::Duration};

#[test]
fn short_sessions_keep_backoff_until_a_stable_session_resets_it() -> crate::Result<()> {
    let mut recovery = Recovery::new(crate::client::backoff().with_jitter_seed(7));
    let first = recovery.after(None)?;
    assert!((Duration::from_millis(250)..Duration::from_millis(500)).contains(&first));
    let second = recovery.after(Some(Duration::from_secs(1)))?;
    assert!((Duration::from_millis(500)..Duration::from_secs(1)).contains(&second));
    let third = recovery.after(Some(Duration::from_secs(59)))?;
    assert!((Duration::from_secs(1)..Duration::from_secs(2)).contains(&third));
    assert_eq!(recovery.after(Some(Duration::from_secs(60)))?, first);
    Ok(())
}

#[test]
fn reconnects_spread_over_the_jitter_window_and_never_exceed_the_cap() -> crate::Result<()> {
    let mut buckets = HashSet::new();
    for seed in 0..100 {
        let mut recovery = Recovery::new(crate::client::backoff().with_jitter_seed(seed));
        let first = recovery.after(None)?;
        assert!(first <= Duration::from_secs(2));
        buckets.insert(first.as_millis() / 50);
        for _ in 0..20 {
            assert!(recovery.after(Some(Duration::from_secs(1)))? <= Duration::from_secs(30));
        }
    }
    assert!(
        buckets.len() >= 3,
        "100 reconnects must occupy several deterministic time buckets"
    );
    Ok(())
}
