//! The inner environment's two calls, each on the blocking pool: `env/renew`,
//! the environment ended and built again, since its teardown waits for every
//! process inside to be reaped; and `env/hosts`, the hosts file it resolves
//! names through replaced.

use std::io;
use std::sync::Arc;

use jsonrpsee_types::Id;

use super::Session;
use crate::error::{self, Result};
use crate::protocol::{HostsParams, RenewParams};
use crate::server::launch::InnerEnvironment;

impl Session {
    /// Renews the inner environment, answering once the new one stands. The
    /// processes the old one ran end with it, and each ending reaches the
    /// supervisor as any process's does.
    pub(super) fn renew(&mut self, id: Id<'static>, params: Result<RenewParams>) {
        self.on_inner(id, params, |inner, _emptied: RenewParams| inner.renew());
    }

    /// Replaces the inner environment's hosts file, answering once every
    /// process inside reads the new one.
    pub(super) fn hosts(&mut self, id: Id<'static>, params: Result<HostsParams<'static>>) {
        self.on_inner(id, params, |inner, hosts: HostsParams<'static>| {
            inner.hosts(&hosts.file)
        });
    }

    /// Runs `call` on the inner environment with its decoded `params`, off
    /// the runtime's threads; refused when the executor builds none.
    fn on_inner<P: Send + 'static>(
        &mut self,
        id: Id<'static>,
        params: Result<P>,
        call: impl FnOnce(&dyn InnerEnvironment, P) -> io::Result<()> + Send + 'static,
    ) {
        let inner = params.and_then(|params| {
            self.inner
                .as_ref()
                .map(|inner| (Arc::clone(inner), params))
                .ok_or_else(error::no_inner_environment)
        });
        match inner {
            Ok((inner, params)) => self.blocking(id, move |_workspace| {
                (
                    call(inner.as_ref(), params).map_err(error::inner_unavailable),
                    None,
                )
            }),
            Err(failure) => self.answer::<()>(id, Err(failure)),
        }
    }
}
