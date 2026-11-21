//! A fake environment: every process it starts plays an MCP server on its
//! pipes, and its browser, when it has one, is [`FakeBrowser`].

use super::*;

mod browser;
mod mcp_failures;
mod mcp_serving;

/// How the fake server behaves.
#[derive(Debug, Default, Clone)]
struct Behaviour {
    /// Answers `initialize` with an error.
    refuse_initialize: bool,
    /// Methods it never answers.
    silent: Vec<&'static str>,
    /// Methods it answers with a JSON-RPC error.
    failing: Vec<&'static str>,
    /// Methods it exits on, unanswered.
    exits_on: Vec<&'static str>,
}

/// An environment whose every process is a fake MCP server, and whose
/// browser, when it has one, is [`FakeBrowser`].
#[derive(Debug, Default)]
struct FakeEnv {
    behaviour: Behaviour,
    browser: Option<FakeBrowser>,
    refuse_spawn: bool,
    feeds: Mutex<HashMap<u64, Feed>>,
    next: AtomicU64,
    spawned: Mutex<Vec<Vec<String>>>,
    killed: Mutex<Vec<u64>>,
    killed_note: tokio::sync::Notify,
}

/// A browser that opens and screenshots, and whose every action fails; a
/// closed one fails its screenshots too.
#[derive(Debug, Default)]
struct FakeBrowser {
    opened: Mutex<Vec<Value>>,
    closed: bool,
}

impl Browser for FakeBrowser {
    fn open<'a>(&'a self, arguments: &'a Value) -> EnvFuture<'a, String> {
        lock(&self.opened).push(arguments.clone());
        Box::pin(std::future::ready(Ok("opened Example Domain".to_owned())))
    }

    fn act<'a>(&'a self, _arguments: &'a Value) -> EnvFuture<'a, String> {
        Box::pin(std::future::ready(Err(EnvError::new(
            EnvErrorKind::Unavailable,
            "the page closed",
        ))))
    }

    fn screenshot<'a>(&'a self, _arguments: &'a Value) -> EnvFuture<'a, Image> {
        if self.closed {
            return Box::pin(std::future::ready(Err(EnvError::new(
                EnvErrorKind::Unavailable,
                "no page is open",
            ))));
        }
        Box::pin(std::future::ready(Ok(Image {
            kind: ImageKind::Png,
            data: Bytes::from_static(b"\x89PNG"),
        })))
    }
}

impl FakeEnv {
    fn serving(behaviour: Behaviour) -> Self {
        Self {
            behaviour,
            ..Self::default()
        }
    }

    /// What the server answers to `method`: its result, or its error.
    fn answer(&self, method: &str, params: &Value) -> Option<Result<Value, Value>> {
        if self.behaviour.silent.contains(&method) {
            return None;
        }
        if self.behaviour.failing.contains(&method) {
            return Some(Err(
                json!({"code": -32_603, "message": format!("{method} broke")}),
            ));
        }
        let result = match method {
            "initialize" if self.behaviour.refuse_initialize => {
                return Some(Err(
                    json!({"code": -32_600, "message": "initialize refused"}),
                ));
            }
            "initialize" => json!({
                "protocolVersion": params.get("protocolVersion").cloned().unwrap_or(Value::Null),
                "capabilities": {"tools": {}, "resources": {}},
                "serverInfo": {"name": "fake", "version": "1.0.0"}
            }),
            "tools/list" => json!({"tools": [
                {"name": "search.v2", "description": "Searches the docs.",
                 "inputSchema": {"type": "object", "properties": {"q": {"type": "string"}}}},
                {"name": "stats", "inputSchema": {"type": "object"}},
                {"name": "explode", "inputSchema": {"type": "object"}}
            ]}),
            "tools/call" => return Some(Ok(call_answer(params))),
            "resources/list" => json!({"resources": [
                {"uri": "file:///guide.md", "name": "guide"}
            ]}),
            "resources/templates/list" => json!({"resourceTemplates": [
                {"uriTemplate": "file:///{path}", "name": "files"}
            ]}),
            "resources/read" => json!({"contents": [
                {"uri": "file:///guide.md", "text": "# Guide"},
                {"uri": "file:///logo.png", "mimeType": "image/png", "blob": "AAAA"},
                {"uri": "file:///raw", "blob": "AAAA"}
            ]}),
            other => {
                return Some(Err(
                    json!({"code": -32_601, "message": format!("{other} is unknown")}),
                ));
            }
        };
        Some(Ok(result))
    }
}

/// What the fake server's tools answer, by the tool called.
fn call_answer(params: &Value) -> Value {
    match params.get("name").and_then(Value::as_str) {
        Some("search.v2") => json!({"content": [
            {"type": "text", "text": format!("found {}", params.get("arguments").and_then(|a| a.get("q")).and_then(Value::as_str).unwrap_or("nothing"))},
            {"type": "image", "data": "AAAA", "mimeType": "image/png"},
            {"type": "resource", "resource": {"uri": "file:///a", "text": "embedded"}},
            {"type": "resource", "resource": {"uri": "file:///b", "blob": "AAAA"}},
            {"type": "resource_link", "uri": "file:///c", "name": "c"}
        ]}),
        Some("stats") => json!({"content": [], "structuredContent": {"pages": 3}}),
        _ => json!({"content": [{"type": "text", "text": "it blew up"}], "isError": true}),
    }
}

impl Env for FakeEnv {
    fn root(&self) -> &'static str {
        "/fake"
    }

    fn spawn<'a>(&'a self, spawn: &'a Spawn) -> EnvFuture<'a, Process> {
        lock(&self.spawned).push(spawn.argv().to_vec());
        if self.refuse_spawn {
            return Box::pin(std::future::ready(Err(EnvError::new(
                EnvErrorKind::Unavailable,
                "no such program",
            ))));
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (feed, piped) = pipe();
        // A server's own log line, on both streams, comes before any message.
        feed.output(Stream::Stderr, Bytes::from_static(b"starting up\n"));
        feed.output(Stream::Stdout, Bytes::from_static(b"fake server ready\n"));
        lock(&self.feeds).insert(id, feed);
        Box::pin(std::future::ready(Ok(Process {
            id: ProcessId::new(id),
            events: Box::new(piped),
        })))
    }

    fn write(&self, process: ProcessId, data: Bytes) -> EnvFuture<'_, ()> {
        let message: Value = serde_json::from_slice(&data).expect("the client writes JSON lines");
        let id = message.get("id").cloned();
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        if self.behaviour.exits_on.contains(&method.as_str()) {
            let feed = lock(&self.feeds).remove(&process.get());
            if let Some(feed) = feed {
                feed.end(Ending::Exited(1), false);
            }
            return Box::pin(std::future::ready(Ok(())));
        }
        if let (Some(id), Some(answer)) = (id, self.answer(&method, &params)) {
            let reply = match answer {
                Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
            };
            let mut line = serde_json::to_vec(&reply).expect("a reply encodes");
            line.push(b'\n');
            let feeds = lock(&self.feeds);
            if let Some(feed) = feeds.get(&process.get()) {
                // Split in two chunks, so a message arrives across reads.
                let (head, tail) = line.split_at(line.len() / 2);
                feed.output(Stream::Stdout, Bytes::copy_from_slice(head));
                feed.output(Stream::Stdout, Bytes::copy_from_slice(tail));
            }
        }
        Box::pin(std::future::ready(Ok(())))
    }

    fn kill(&self, process: ProcessId) -> EnvFuture<'_, ()> {
        let feed = lock(&self.feeds).remove(&process.get());
        if let Some(feed) = feed {
            feed.end(Ending::Signaled(9), false);
        }
        lock(&self.killed).push(process.get());
        self.killed_note.notify_one();
        Box::pin(std::future::ready(Ok(())))
    }

    fn read_file<'a>(&'a self, _path: &'a str, _max_bytes: u64) -> EnvFuture<'a, FileContent> {
        Box::pin(std::future::ready(Err(EnvError::new(
            EnvErrorKind::Unavailable,
            "no files",
        ))))
    }

    fn write_file<'a>(&'a self, _path: &'a str, _data: Bytes) -> EnvFuture<'a, ()> {
        Box::pin(std::future::ready(Err(EnvError::new(
            EnvErrorKind::Unavailable,
            "no files",
        ))))
    }

    fn append_file<'a>(&'a self, _path: &'a str, _data: Bytes) -> EnvFuture<'a, ()> {
        Box::pin(std::future::ready(Err(EnvError::new(
            EnvErrorKind::Unavailable,
            "no files",
        ))))
    }

    fn delete_file<'a>(&'a self, _path: &'a str) -> EnvFuture<'a, ()> {
        Box::pin(std::future::ready(Err(EnvError::new(
            EnvErrorKind::Unavailable,
            "no files",
        ))))
    }

    fn list_dir<'a>(&'a self, _path: &'a str) -> EnvFuture<'a, Listing> {
        Box::pin(std::future::ready(Err(EnvError::new(
            EnvErrorKind::Unavailable,
            "no files",
        ))))
    }

    fn browser(&self) -> Result<&dyn Browser, &str> {
        self.browser
            .as_ref()
            .map(|browser| browser as &dyn Browser)
            .ok_or("has no browser here")
    }
}

/// Starts the fake server `env` plays, named `docs`.
async fn started(env: &Arc<FakeEnv>) -> Arc<Server> {
    let spawn = Spawn::program("docs-mcp").arg("--stdio");
    let server = Server::stdio("docs", Arc::<FakeEnv>::clone(env), &spawn)
        .await
        .expect("the server initializes");
    Arc::new(server)
}

/// The server's tools and the three resource tools, as one run holds them.
fn mcp_tools<'run>(
    server: &Arc<Server>,
    offered: Vec<Box<ContextTool<'run>>>,
) -> Tools<ContextTool<'run>> {
    let servers: Vec<Arc<Server>> = vec![Arc::clone(server)];
    let mut tools = Tools::new(offered);
    tools.offer(Box::new(Typed::new(ListMcpResources::new(servers.clone()))));
    tools.offer(Box::new(Typed::new(ListMcpResourceTemplates::new(
        servers.clone(),
    ))));
    tools.offer(Box::new(Typed::new(ReadMcpResource::new(servers))));
    tools
}
