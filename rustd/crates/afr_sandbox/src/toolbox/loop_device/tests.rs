use std::cell::Cell;
use std::io;

use super::{
    ATTACH_RETRIES, Access, Attempt, LO_FLAGS_DIRECT_IO, LO_FLAGS_READ_ONLY, caching, until_free,
};
use crate::workspace_disk::Caching;

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

/// A device whose status lacks the direct-I/O flag is buffered, whatever the
/// attach asked for: the kernel may drop direct I/O without failing the call,
/// so only the status read back says how the image is cached.
#[test]
fn test_disk_reports_buffered_fallback() {
    assert_eq!(caching(LO_FLAGS_READ_ONLY), Caching::Buffered);
    assert_eq!(caching(0), Caching::Buffered);
    assert_eq!(caching(LO_FLAGS_DIRECT_IO), Caching::Direct);
    assert_eq!(
        caching(LO_FLAGS_READ_ONLY | LO_FLAGS_DIRECT_IO),
        Caching::Direct
    );
}

/// Only the toolbox is attached read-only; a lease's disk takes writes.
#[test]
fn test_only_the_toolbox_is_attached_read_only() {
    assert_eq!(Access::ReadOnly.flags(), LO_FLAGS_READ_ONLY);
    assert_eq!(Access::ReadWrite.flags(), 0);
}
