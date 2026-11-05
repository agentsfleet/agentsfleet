//! Production admission, with pointed fixture selection like the fleet lease suite.

mod capacity;
mod context;
mod discovery;
mod failure;
mod fixture;
mod funnel;
mod history;
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
mod unreadable;

use super::{
    fixture::{Fixture, now},
    mint,
};
use fixture::{claim, funded, queue};
