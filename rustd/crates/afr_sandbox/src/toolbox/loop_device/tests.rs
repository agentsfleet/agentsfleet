#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::cell::Cell;
use std::fs::{self, File};
use std::io;
use std::path::Path;

use super::{
    ATTACH_RETRIES, Access, Attempt, LO_FLAGS_DIRECT_IO, LO_FLAGS_READ_ONLY, LoopConfig,
    LoopInfo64, caching, configure_free, direct_or_buffered, node_in, status, until_free,
};
use crate::host::BLOCK_BYTES;
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
    // The kernel makes a device read-only when its configuring handle
    // cannot write, so only the workspace disk's device opens for writing.
    assert!(!Access::ReadOnly.writes() && Access::ReadWrite.writes());
    // Only the workspace disk reads past the host's cache.
    assert!(!Access::ReadOnly.direct_io() && Access::ReadWrite.direct_io());
}

/// Direct I/O first, and the buffered attach only when the kernel refuses
/// direct I/O for this file: any other refusal is the answer as it stands.
#[test]
fn should_fall_back_to_buffered_only_when_direct_io_is_refused() {
    let buffered = Cell::new(false);
    let attach = |direct: io::Result<()>| {
        direct_or_buffered(
            || direct,
            || {
                buffered.set(true);
                Ok(())
            },
        )
    };

    attach(Err(io::Error::from_raw_os_error(libc::EINVAL))).unwrap();
    assert!(
        buffered.replace(false),
        "refused direct I/O, attached buffered"
    );
    attach(Err(io::Error::from_raw_os_error(libc::EBUSY))).unwrap_err();
    attach(Ok(())).unwrap();
    assert!(!buffered.get(), "nothing else falls back");
}

/// A descriptor that is no loop device fails the control call and the status
/// call alike, as a failure and never a device taken.
#[test]
fn should_fail_on_a_descriptor_that_is_no_loop_device() {
    let null = File::open("/dev/null").unwrap();
    let config = LoopConfig {
        fd: 0,
        block_size: BLOCK_BYTES,
        info: LoopInfo64::ZERO,
        reserved: [0; 8],
    };

    let refused = configure_free(&null, &config, Access::ReadOnly);

    assert!(matches!(refused, Err(Attempt::Failed(_))));
    status(&null).unwrap_err();
}

/// A device is named by its `uevent`'s `DEVNAME`; one that lists no name, or
/// is not listed at all, names no node.
#[test]
fn should_name_a_device_from_its_uevent() {
    let sys = tempfile::tempdir().unwrap();
    for (number, uevent) in [
        ("7:1", "MAJOR=7\nMINOR=1\nDEVNAME=loop1\n"),
        ("7:2", "MAJOR=7\nMINOR=2\n"),
    ] {
        fs::create_dir(sys.path().join(number)).unwrap();
        fs::write(sys.path().join(number).join("uevent"), uevent).unwrap();
    }

    assert_eq!(node_in(sys.path(), 7, 1).unwrap(), Path::new("/dev/loop1"));
    // No name listed, then no such device.
    node_in(sys.path(), 7, 2).unwrap_err();
    node_in(sys.path(), 7, 3).unwrap_err();
}
