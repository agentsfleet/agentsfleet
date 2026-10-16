//! Recovery uses real expiry, receipts, replay tombstones and private rows.

mod interruption;
mod retention;

use super::{
    attempt::Attempt,
    fixture::{Fixture, now},
    mint,
};
