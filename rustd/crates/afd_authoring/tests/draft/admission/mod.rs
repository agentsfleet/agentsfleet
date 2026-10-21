//! Production admission, with pointed fixture selection like the fleet lease suite.

mod capacity;
mod context;
mod discovery;
mod failure;
mod fixture;
mod funnel;
mod history;
mod holds;
mod identity;
mod obligations;
mod offered;
mod platform;
mod recovery;
mod rejections;
mod replacement;
mod revalidation;
mod skipping;
mod stop;

use super::{
    fixture::{Fixture, now},
    mint,
};
use fixture::{claim, funded, queue};
