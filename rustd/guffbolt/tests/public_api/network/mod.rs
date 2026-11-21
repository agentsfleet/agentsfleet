//! The tools that reach past the machine, driven through their public
//! seams: `http_request` and `web_fetch` over an `Outbound` hook, the
//! standalone `LocalOutbound` and its refusals, an MCP server over the run's
//! `Env`, and the browser tools over an environment's browser.
//!
//! Nothing here reaches the network. A fake hook stands at the `Outbound`
//! boundary, a fake lookup answers every name, and a fake environment plays
//! an MCP server over the pipes a process would have.

use std::collections::HashMap;
use std::error::Error as StdError;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bytes::Bytes;
use guffbolt::tools::env::{
    Browser, Ending, Env, EnvError, EnvErrorKind, EnvFuture, Feed, FileContent, Listing, Process,
    ProcessId, Spawn, Stream, pipe,
};
use guffbolt::tools::mcp::{
    ListMcpResourceTemplates, ListMcpResources, ReadMcpResource, Server, tool_name,
};
use guffbolt::tools::outbound::{
    Admitted, Allowlist, LocalOutbound, Outbound, OutboundFuture, Placement, Redirects, Refused,
    Request, Response,
};
use guffbolt::tools::{Context, ContextTool, ToolsConfig, Typed, code};
use guffbolt::{CallRef, Image, ImageKind, ToolOutput, Tools};
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use serde_json::{Value, json};

mod fake_env;
mod http_request;
mod local_outbound;
mod mcp_http;
mod web_fetch;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs `name` from `tools` with `arguments`, as call 0 of step 1.
async fn dispatch(tools: &Tools<ContextTool<'_>>, name: &str, arguments: Value) -> ToolOutput {
    let call_ref = CallRef {
        step: 1,
        index: 0,
        call_id: "call-1".to_owned(),
    };
    let context = Context {
        call_ref: &call_ref,
        run_id: "run-1",
        wake: None,
    };
    tools.dispatch(name, &arguments, context).await
}

/// The tools `names` builds over `outbound`.
fn web<'run>(outbound: &'run dyn Outbound, names: &[&str]) -> Tools<ContextTool<'run>> {
    let config = ToolsConfig::new(names).expect("names");
    Tools::builder()
        .with_outbound(outbound)
        .build(&config)
        .expect("an outbound is given")
}

// A hook that answers every request with one canned answer.

/// What a canned hook says the secret it placed is.
const PLACED: &str = "tok-123";

/// A hook that answers each request with `answer`, masks [`PLACED`], and
/// records every request and every mask it was asked for. It keeps the
/// trait's own lookup.
#[derive(Debug)]
struct Canned {
    answer: Result<Response, Refused>,
    sent: Mutex<Vec<(String, Request)>>,
    masked: AtomicU64,
}

impl Canned {
    fn answering(answer: Result<Response, Refused>) -> Self {
        Self {
            answer,
            sent: Mutex::new(Vec::new()),
            masked: AtomicU64::new(0),
        }
    }

    fn ok(response: Response) -> Self {
        Self::answering(Ok(response))
    }

    fn sent(&self) -> Vec<(String, Request)> {
        lock(&self.sent).clone()
    }
}

impl Outbound for Canned {
    fn send<'a>(&'a self, tool: &'a str, request: Request) -> OutboundFuture<'a, Response> {
        lock(&self.sent).push((tool.to_owned(), request));
        Box::pin(std::future::ready(self.answer.clone()))
    }

    fn mask(&self, text: String) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
        self.masked.fetch_add(1, Ordering::SeqCst);
        Box::pin(std::future::ready(text.replace(PLACED, "[masked]")))
    }

    fn secret<'a>(
        &'a self,
        _tool: &'a str,
        name: &'a str,
        _field: &'a str,
    ) -> OutboundFuture<'a, String> {
        Box::pin(std::future::ready(Err(Refused::new(
            code::SECRET_NOT_FOUND,
            format!("no secret {name}"),
        ))))
    }

    fn connect<'a>(&'a self, _tool: &'a str, url: &'a str) -> OutboundFuture<'a, Admitted> {
        Box::pin(std::future::ready(Ok(Admitted {
            url: url.to_owned(),
            headers: Vec::new(),
        })))
    }
}

fn response(status: u16, body: &str) -> Response {
    Response {
        status,
        body: body.to_owned(),
        ..Response::default()
    }
}

/// A lookup with fixed answers, refusing any other name, recording each name
/// it was asked.
#[derive(Debug, Default)]
struct Fixed {
    answers: HashMap<String, Vec<SocketAddr>>,
    asked: Mutex<Vec<String>>,
}

impl Fixed {
    fn answering(mut self, name: &str, address: &str) -> Self {
        let address: SocketAddr = address.parse().expect("an address");
        self.answers.insert(name.to_owned(), vec![address]);
        self
    }

    fn asked(&self) -> Vec<String> {
        lock(&self.asked).clone()
    }
}

impl Resolve for Fixed {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        lock(&self.asked).push(host.clone());
        let answer: Result<Addrs, Box<dyn StdError + Send + Sync>> =
            self.answers.get(&host).map_or_else(
                || Err(format!("no answer for {host}").into()),
                |addresses| Ok(Box::new(addresses.clone().into_iter()) as Addrs),
            );
        Box::pin(std::future::ready(answer))
    }
}
