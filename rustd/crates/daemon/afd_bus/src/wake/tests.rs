use af_core::id::Uuid7;

use super::{DEFAULT_WAKE_QUEUE_CAPACITY as CAPACITY, Wake};

type TestResult<T = ()> = crate::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn fleet(number: usize) -> TestResult<Uuid7> {
    Ok(Uuid7::parse(&format!(
        "0195b4ba-8d3a-7000-8000-{number:012x}"
    ))?)
}

#[test]
fn configured_capacity_is_applied_and_invalid_settings_fail() -> TestResult {
    use af_core::env::MapEnv;
    for raw in ["0", "65537", "-1", "not-a-number"] {
        let env = MapEnv::from_pairs([(super::ENV_WAKE_QUEUE_CAPACITY, raw)]);
        let Err(_) = Wake::from_env(&env) else {
            return Err("invalid capacity accepted".into());
        };
    }
    let env = MapEnv::from_pairs([(super::ENV_WAKE_QUEUE_CAPACITY, "2")]);
    let wake = Wake::from_env(&env)?;
    for id in 0..3 {
        wake.ring(fleet(id)?);
    }
    assert_eq!(wake.take(), Some(fleet(1)?));
    assert_eq!(wake.take(), Some(fleet(2)?));
    assert_eq!(wake.take(), None);
    let default = Wake::from_env(&MapEnv::default())?;
    default.ring(fleet(0)?);
    assert_eq!(default.take(), Some(fleet(0)?));
    Ok(())
}

#[test]
fn a_thousand_fleet_notifications_remain_bounded_and_consumed_fleets_can_ring_again() -> TestResult
{
    let wake = Wake::with_capacity(64)?;
    for id in 0..1000 {
        wake.ring(fleet(id)?);
    }
    for id in 936..1000 {
        assert_eq!(wake.take(), Some(fleet(id)?));
    }
    assert_eq!(wake.take(), None);
    wake.ring(fleet(999)?);
    wake.ring(fleet(999)?);
    assert_eq!(wake.take(), Some(fleet(999)?));
    assert_eq!(wake.take(), None);
    Ok(())
}

#[test]
fn a_poisoned_wake_queue_is_reset_before_reuse() -> TestResult {
    let wake = Wake::with_capacity(2)?;
    wake.ring(fleet(1)?);
    let poisoned = wake.clone();
    let worker = std::thread::spawn(move || {
        let Ok(_held) = poisoned.queue.lock() else {
            return;
        };
        std::panic::resume_unwind(Box::new("queue worker failure fixture"));
    });
    let Err(_) = worker.join() else {
        return Err("poison fixture did not fire".into());
    };
    wake.ring(fleet(2)?);
    assert_eq!(wake.take(), Some(fleet(2)?));
    assert_eq!(wake.take(), None);
    wake.ring(fleet(2)?);
    assert_eq!(wake.take(), Some(fleet(2)?));
    Ok(())
}

struct Observer {
    wake: Wake,
    queued: std::sync::atomic::AtomicBool,
}

impl std::task::Wake for Observer {
    fn wake(self: std::sync::Arc<Self>) {
        self.queued.store(
            self.wake.take().is_some(),
            std::sync::atomic::Ordering::SeqCst,
        );
    }
}

#[tokio::test]
async fn a_notified_session_can_already_consume_the_published_fleet()
-> crate::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let wake = Wake::default();
    let mut generations = wake.generations();
    let observed = std::sync::Arc::new(Observer {
        wake: wake.clone(),
        queued: std::sync::atomic::AtomicBool::new(false),
    });
    let waker = std::task::Waker::from(std::sync::Arc::clone(&observed));
    let mut changed = Box::pin(generations.changed());
    assert!(
        changed
            .as_mut()
            .poll(&mut std::task::Context::from_waker(&waker))
            .is_pending()
    );
    wake.ring(Uuid7::parse("0195b4ba-8d3a-7000-8000-000000000001")?);
    assert!(
        observed.queued.load(std::sync::atomic::Ordering::SeqCst),
        "notification must follow insertion, or the session misses the notification"
    );
    Ok(())
}

#[test]
fn bounded_queue_keeps_recent_fleets_and_a_duplicate_still_advances_the_generation()
-> crate::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let wake = Wake::default();
    let observed = wake.generations();
    for number in 0..=CAPACITY {
        let fleet = Uuid7::parse(&format!("0195b4ba-8d3a-7000-8000-{number:012x}"))?;
        wake.ring(fleet);
    }
    let newest = Uuid7::parse(&format!("0195b4ba-8d3a-7000-8000-{CAPACITY:012x}"))?;
    wake.ring(newest.clone());
    assert_eq!(*observed.borrow(), 66);
    let mut remaining = Vec::new();
    while let Some(fleet) = wake.take() {
        remaining.push(fleet);
    }
    assert_eq!(remaining.len(), CAPACITY);
    assert_eq!(
        remaining.first().cloned(),
        Some(Uuid7::parse("0195b4ba-8d3a-7000-8000-000000000001")?)
    );
    assert_eq!(remaining.last().cloned(), Some(newest));
    Ok(())
}
