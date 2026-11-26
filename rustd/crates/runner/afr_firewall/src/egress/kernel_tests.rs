#![expect(clippy::panic, reason = "the test is of a thread that panics")]

use std::io;

use super::on_own_thread;

/// Work sent into another namespace runs on a thread of its own and hands its
/// answer back; a thread that panics there is a failure, not a crash.
#[test]
fn test_work_on_its_own_thread_comes_back() {
    let caller = std::thread::current().id();

    let ran_on = on_own_thread(|| Ok::<_, io::Error>(std::thread::current().id()));
    let panicked = on_own_thread(|| -> io::Result<()> { panic!("inside a namespace") });

    assert_ne!(ran_on.ok(), Some(caller));
    assert_eq!(
        panicked.err().map(|error| error.kind()),
        Some(io::ErrorKind::Other)
    );
}
