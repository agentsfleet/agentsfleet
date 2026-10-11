use std::cell::Cell;
use std::io;

use super::{ATTACH_RETRIES, Attempt, until_free};

/// A `LOOP_CONFIGURE` refusal with `errno`.
fn configure(errno: i32) -> Attempt {
    Attempt::configure(io::Error::from_raw_os_error(errno))
}

/// Only a device taken between the kernel offering it and this runner
/// configuring it is worth another: the same `EBUSY` from any other call, or
/// any other errno from the configure, is final.
#[test]
fn should_try_another_device_only_when_the_configure_found_it_taken() {
    assert!(configure(libc::EBUSY).taken());
    assert!(!configure(libc::EINVAL).taken());
    assert!(!Attempt::Failed(io::Error::from_raw_os_error(libc::EBUSY)).taken());
}

/// A device taken every time is tried once and then once per retry, and the
/// last refusal is the one the caller reads.
#[test]
fn should_give_up_after_the_last_retry_with_the_last_refusal() {
    let tries = Cell::new(0_usize);

    let refused = until_free(|| -> Result<(), Attempt> {
        tries.set(tries.get() + 1);
        Err(configure(libc::EBUSY))
    });

    assert_eq!(tries.get(), ATTACH_RETRIES + 1);
    assert_eq!(
        refused.err().and_then(|error| error.raw_os_error()),
        Some(libc::EBUSY)
    );
}

/// A final failure ends the attach on the try that met it.
#[test]
fn should_stop_at_once_on_a_final_failure() {
    let tries = Cell::new(0_usize);

    let refused = until_free(|| -> Result<(), Attempt> {
        tries.set(tries.get() + 1);
        Err(Attempt::Failed(io::Error::from_raw_os_error(libc::EBUSY)))
    });

    assert_eq!(tries.get(), 1);
    assert_eq!(
        refused.err().and_then(|error| error.raw_os_error()),
        Some(libc::EBUSY)
    );
}

/// A device found taken is followed by another, which attaches.
#[test]
fn should_attach_the_next_device_after_one_found_taken() {
    let tries = Cell::new(0_usize);

    let attached = until_free(|| {
        tries.set(tries.get() + 1);
        if tries.get() == 1 {
            Err(configure(libc::EBUSY))
        } else {
            Ok(tries.get())
        }
    });

    assert_eq!(attached.ok(), Some(2));
}
