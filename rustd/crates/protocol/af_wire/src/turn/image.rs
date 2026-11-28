//! A person's image attachment as stored next to the message.

use serde::{Deserialize, Serialize};
use std::fmt;

/// An image attached to a person or API client's message.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageImage {
    /// One of `image/png`, `image/jpeg`, `image/gif` and `image/webp`.
    pub media_type: String,
    /// Standard base64; at most 5 MiB when decoded.
    pub data: String,
}

impl fmt::Debug for MessageImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MessageImage")
            .field("encoded_bytes", &self.data.len())
            .finish_non_exhaustive()
    }
}

/// Images from a previously validated chat request in the turn ledger.
#[must_use]
pub fn images_of(request_json: &str) -> Vec<MessageImage> {
    #[derive(Deserialize)]
    struct Attachments {
        #[serde(default)]
        images: Vec<MessageImage>,
    }
    serde_json::from_str::<Attachments>(request_json).map_or_default(|request| request.images)
}

/// Whether a previously validated chat request carries images.
#[must_use]
pub fn has_images(request_json: &str) -> bool {
    #[derive(Deserialize)]
    struct Attachments {
        #[serde(default)]
        images: Vec<serde::de::IgnoredAny>,
    }
    serde_json::from_str::<Attachments>(request_json)
        .is_ok_and(|request| !request.images.is_empty())
}
