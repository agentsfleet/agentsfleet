//! A tool the builder built over its environment or network: lent the
//! builder's own for every call, whatever the call's context carries.

use serde_json::Value;

use crate::tool::{Describe, Tool, ToolFuture, ToolSpec};
use crate::tools::{Context, Env, Outbound};

/// `tool`, lent the environment and network it was built over.
#[derive(Debug)]
pub(super) struct Seamed<'run, T> {
    tool: T,
    env: Option<&'run dyn Env>,
    outbound: Option<&'run dyn Outbound>,
}

impl<'run, T> Seamed<'run, T> {
    /// `tool`, running in `env`.
    pub(super) const fn over_env(env: &'run dyn Env, tool: T) -> Self {
        Self {
            tool,
            env: Some(env),
            outbound: None,
        }
    }

    /// `tool`, sending through `outbound`.
    pub(super) const fn over_outbound(outbound: &'run dyn Outbound, tool: T) -> Self {
        Self {
            tool,
            env: None,
            outbound: Some(outbound),
        }
    }
}

impl<T: Describe> Describe for Seamed<'_, T> {
    fn spec(&self) -> ToolSpec<'_> {
        self.tool.spec()
    }
}

impl<'c, T> Tool<Context<'c>> for Seamed<'_, T>
where
    T: for<'x> Tool<Context<'x>>,
{
    fn call<'a>(&'a self, arguments: &'a Value, cx: Context<'c>) -> ToolFuture<'a>
    where
        Context<'c>: 'a,
    {
        let cx: Context<'a> = cx;
        let lent = Context {
            env: self.env.or(cx.env),
            outbound: self.outbound.unwrap_or(cx.outbound),
            ..cx
        };
        self.tool.call(arguments, lent)
    }
}
