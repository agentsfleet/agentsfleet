//! Which loop device is the admitted image: its file's device and inode, read
//! only, from the top, all of it.

use super::backing_mismatch;
use crate::toolbox::loop_device::{Backing, LO_FLAGS_READ_ONLY};

/// The admitted image's device and inode.
const IMAGE: (u64, u64) = (0x0803, 4242);

/// A loop device backed by `file`, otherwise as admission attaches one.
fn attached(file: (u64, u64)) -> Backing {
    Backing {
        file,
        flags: LO_FLAGS_READ_ONLY,
        offset: 0,
        size_limit: 0,
    }
}

#[test]
fn should_take_a_read_only_loop_device_of_the_whole_admitted_file() {
    assert_eq!(backing_mismatch(&attached(IMAGE), IMAGE), None);
}

#[test]
fn should_refuse_a_loop_device_of_another_file() {
    let other_inode = attached((IMAGE.0, IMAGE.1 + 1));
    let other_device = attached((rustix::fs::makedev(9, 3), IMAGE.1));

    for backing in [other_inode, other_device] {
        assert_eq!(
            backing_mismatch(&backing, IMAGE).as_deref(),
            Some("a loop device of another file"),
            "{backing:?}"
        );
    }
}

#[test]
fn should_refuse_a_writable_loop_device_or_one_showing_part_of_its_file() {
    let writable = Backing {
        flags: 0,
        ..attached(IMAGE)
    };
    let offset = Backing {
        offset: 512,
        ..attached(IMAGE)
    };
    let cut = Backing {
        size_limit: 4096,
        ..attached(IMAGE)
    };

    for backing in [writable, offset, cut] {
        assert_eq!(
            backing_mismatch(&backing, IMAGE).as_deref(),
            Some("a loop device that is writable or shows part of its file"),
            "{backing:?}"
        );
    }
}
