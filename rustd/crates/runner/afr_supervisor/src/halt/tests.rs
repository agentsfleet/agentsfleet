use tokio_util::sync::CancellationToken;

use super::Halt;
use crate::client::Verb;
use crate::error;

#[test]
fn shutdown_stops_serving_and_leasing_but_not_running_leases() {
    let shutdown = CancellationToken::new();
    let halt = Halt::new(shutdown.clone());

    shutdown.cancel();

    assert!(halt.serving().is_cancelled() && halt.leasing().is_cancelled());
    assert!(
        !halt.stopped().is_cancelled(),
        "shutdown still permits final delivery"
    );
    assert!(
        !halt.running().is_cancelled(),
        "leases in flight run to their reports"
    );
}

#[test]
fn stopping_leasing_leaves_the_heartbeat_serving() {
    let halt = Halt::new(CancellationToken::new());

    halt.leasing().cancel();

    assert!(halt.leasing().is_cancelled());
    assert!(!halt.serving().is_cancelled() && !halt.running().is_cancelled());
}

#[test]
fn a_refused_token_stops_everything_and_other_failures_do_not() {
    let halt = Halt::new(CancellationToken::new());

    let busy = halt.stops_on(&error::unavailable(Verb::Heartbeat, 503));
    let forbidden = halt.stops_on(&error::refused(Verb::Heartbeat, 403, None));
    assert!(!busy && !forbidden && !halt.serving().is_cancelled());
    assert!(!halt.token_refused());

    assert!(halt.stops_on(&error::refused(Verb::Heartbeat, 401, None)));
    assert!(
        halt.stops_on(&error::refused(Verb::Report, 401, None)),
        "a second refusal still stops"
    );

    assert!(halt.token_refused());
    assert!(halt.running().is_cancelled() && halt.serving().is_cancelled());
    assert!(halt.leasing().is_cancelled());
    assert!(halt.stopped().is_cancelled());
}
