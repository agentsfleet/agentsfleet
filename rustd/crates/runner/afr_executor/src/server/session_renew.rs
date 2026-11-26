//! `env/renew`: the inner environment ended and built again, on the blocking
//! pool, since its teardown waits for every process inside to be reaped.

use std::sync::Arc;

use jsonrpsee_types::Id;

use super::Session;
use crate::error::{self, Result};
use crate::protocol::RenewParams;

impl Session {
    /// Renews the inner environment, answering once the new one stands. The
    /// processes the old one ran end with it, and each ending reaches the
    /// supervisor as any process's does.
    pub(super) fn renew(&mut self, id: Id<'static>, params: Result<RenewParams>) {
        let inner = params.and_then(|_emptied: RenewParams| {
            self.inner
                .as_ref()
                .map(Arc::clone)
                .ok_or_else(error::no_inner_environment)
        });
        match inner {
            Ok(inner) => self.blocking(id, move |_workspace| {
                (inner.renew().map_err(error::inner_unavailable), None)
            }),
            Err(failure) => self.answer::<()>(id, Err(failure)),
        }
    }
}
