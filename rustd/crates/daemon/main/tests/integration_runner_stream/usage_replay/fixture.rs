//! Two real fleet runs share one production runner renewal ticker.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions fail loudly"
)]

use crate::e2e::Scenario;
use af_core::{env::MapEnv, id::Uuid7, timing::STREAM_ANSWER_TIMEOUT};
use af_wire::{protocol::Down, run::TaggedRun};
use afr_agent::Meter;
use afr_supervisor::{config, test_util::Session};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub(super) use super::super::population::sibling;

pub(super) struct Held {
    pub(super) run: Uuid7,
    pub(super) event: Uuid7,
    pub(super) fence: u64,
    expires: jiff::Timestamp,
}

pub(super) fn config(scene: &Scenario, origin: &str, root: &std::path::Path) -> config::Config {
    let root = root.to_string_lossy().into_owned();
    config::Config::from_env(&MapEnv::from_pairs([
        (config::ENV_API_URL, scene.base.as_str()),
        (config::ENV_RUNNER_TOKEN, scene.token.as_str()),
        (config::ENV_STREAM_URL, origin),
        (config::ENV_STREAM_CA, root.as_str()),
    ]))
    .expect("native test configuration")
}

pub(super) async fn offers(session: &Session, first: &Uuid7, second: &Uuid7) -> [Held; 2] {
    let mut held = Vec::new();
    for _ in 0..2 {
        let frame = tokio::time::timeout(STREAM_ANSWER_TIMEOUT * 2, session.offer())
            .await
            .expect("pushed offer deadline")
            .expect("pushed offer");
        let message: Down<'_> = frame.decode().expect("strict native offer");
        let Down::Run {
            run: TaggedRun::Fleet(run),
        } = message
        else {
            panic!("fleet work expected");
        };
        held.push(Held {
            run: Uuid7::parse(&run.run_id).expect("run UUIDv7"),
            event: Uuid7::parse(&run.event.turn_id).expect("event UUIDv7"),
            fence: run.fencing_token,
            expires: run.run_expires_at,
        });
    }
    held.sort_by_key(|run| i32::from(&run.event != first));
    let held: [Held; 2] = held.try_into().ok().expect("two runs");
    assert_eq!(&held[0].event, first);
    assert_eq!(&held[1].event, second);
    held
}

pub(super) fn meter(lines: usize, input: u64) -> Arc<Meter> {
    let meter = Arc::new(Meter::default());
    for _ in 0..lines {
        meter.add(guffbolt::Usage {
            input,
            output: 10,
            ..guffbolt::Usage::default()
        });
    }
    meter
}

pub(super) fn keep(
    session: &Arc<Session>,
    runs: &[Held; 2],
    meters: &[Arc<Meter>; 2],
    stop: &CancellationToken,
) -> Vec<tokio::task::JoinHandle<()>> {
    runs.iter().zip(meters).map(|(run, meter)| {
        let (session, id, expires, meter, stop) = (Arc::clone(session), run.run.clone(), run.expires, Arc::clone(meter), stop.clone());
        tokio::spawn(async move {
            tokio::select! {
                () = stop.cancelled() => {},
                reason = session.keep_usage(&id, expires, &meter) => panic!("usage owner ended before settlement: {reason:?}"),
            }
        })
    }).collect()
}
