//! The exact price time survives funding delays, retries and late receipts.

use af_wire::usage::UsageLine;
use afd_billing::{
    Posture,
    price::{PriceBook, RatedLine, Rater, Window},
};
use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct FrozenQuote {
    pub at: jiff::Timestamp,
    pub book: PriceBook,
}

impl FrozenQuote {
    pub(super) fn rate(&self, line: &UsageLine) -> Result<RatedLine> {
        Ok(Rater::new(
            &self.book,
            Posture::Platform,
            Window {
                from: self.at,
                until: self.at,
            },
        )
        .rate(line)?)
    }
}
