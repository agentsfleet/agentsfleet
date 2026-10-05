//! The image a call read, moved from the lease onto the call's result.
//!
//! The tool crate and the provider crate each name the four kinds the wires
//! take; the loop is where the two meet, so it is where one becomes the other.

use afr_providers::{ImageInput, ImageKind};
use afr_tools::sandbox::{ImageAttachment, ImageKind as Attached};

/// `attached` as the provider seam carries it.
pub(super) fn image_input(attached: ImageAttachment) -> ImageInput {
    ImageInput {
        kind: match attached.kind {
            Attached::Png => ImageKind::Png,
            Attached::Jpeg => ImageKind::Jpeg,
            Attached::Gif => ImageKind::Gif,
            Attached::Webp => ImageKind::Webp,
        },
        data: attached.bytes,
    }
}
