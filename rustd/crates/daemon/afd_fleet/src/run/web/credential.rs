//! Each dispatch reads a platform-owned bag, scoped to the administrator
//! workspace. The key itself is wiped when the dispatch releases it.

use af_crypto::secret::SecretString;
use afd_vault::bag::{self, Bag, BagFault};

use super::{RunService, VendorName};
use crate::Result;

struct WebKey(SecretString);

#[derive(serde::Deserialize)]
struct Stored {
    api_key: Option<SecretString>,
}

impl Bag for WebKey {
    type Stored = Stored;

    fn from_stored(stored: Stored) -> core::result::Result<Self, BagFault> {
        bag::secret(stored.api_key, "api_key").map(Self)
    }
}

impl RunService {
    pub(super) async fn web_key(&self, vendor: VendorName) -> Result<Option<SecretString>> {
        let name = vendor.name();
        let read = self
            .vault
            .bag::<WebKey>(self.runs.web.admin.as_ref(), name)
            .await?;
        let key = bag::usable(
            read,
            name,
            "web_vendor_key_unusable",
            af_core::error_code::WEB_VENDOR_UNCONFIGURED.as_str(),
        );
        Ok(key.map(|key| key.0))
    }
}
