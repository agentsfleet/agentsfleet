//! Omitted optional fields are absent; an explicit null still has to match T.

use serde::{Deserialize, Deserializer};

pub(crate) fn present<'de, T, D>(deserializer: D) -> crate::Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}
