# Code

This standard covers source code in every language: dead code and legacy, types, errors, resources, untrusted input, size, and scripted Git edits.
A coding agent reads it before editing, and a reviewer judges a diff against it.

## Dead code and legacy

### NDC — Code carries no unused variable, import, parameter or unreachable branch.
- **Applies to:** all source files
- **Checked by:** linter: rustc unused lint group; oxlint no-unused-vars
- **Bad:** `import { readFile, writeFile } from "node:fs";` with only `readFile` called
- **Good:** `import { readFile } from "node:fs";`
- **Why:** an unused symbol misleads readers about what the code depends on.

### NRC — A comment explains why the code is the way it is, never what the next line does.
- **Applies to:** comments in all source files
- **Checked by:** review
- **Bad:** `// increment the retry count` above `retries += 1;`
- **Good:** `// Drained before release because the pool hands this connection to the next request.`
- **Why:** a comment that restates the code drifts from it and teaches readers to skip comments.

A comment is written for a reader new to the codebase, and names any sibling file or registry its correctness depends on.
It carries no review context: no change history, no spec lineage, no claim that the diff is correct.

### NLR — A diff that touches a file removes the dead code and legacy framing already in it.
- **Applies to:** every file a diff edits, read whole; legacy framing means `legacy_*` names, `V2` twins, `// legacy` comments and deprecation warnings
- **Checked by:** review
- **Bad:** an edit to `worker.rs` that keeps `reload: Option<Sender>` although every caller passes `Some`
- **Good:** the same diff makes the field `Sender` and deletes the dead `None` branch
- **Why:** the agent already reading the file is the cheapest one to remove its rot.

A cleanup over about 200 net lines, or keeping, patching or removing a whole legacy subsystem, is the owner's call before the edit.
Designing around a dirty file, or wrapping it in a shim, keeps the legacy and needs the same call.
A behaviour-preserving change folds in no performance rewrite; that is raised separately.

### NLG — An interface changes in place, with every caller updated in the same commit and no alias, twin or fallback beside it.
- **Applies to:** routes, remote procedure calls, types, tables, commands, flags, configuration keys and environment variables
- **Checked by:** review
- **Bad:** `CreateRunV2` beside `CreateRun`, or `add` kept as an alias after the verb became `create`
- **Good:** `CreateRun` changes in place and every caller updates in the same commit
- **Why:** a duplicate surface is a commitment nobody owns that every later change has to reason about.

A compatibility surface needs an owner decision naming the outside caller and its migration, judged by this repository's own version policy.
A list that catalogues violations for later, such as `LEGACY_` or `TO_FIX_`, is itself a violation.
A name an outside party pins, such as an OAuth callback path, takes a `VENDOR_` or `EXTERNAL_` prefix.

### HLP — A helper, function or exported item ships in the same change as its first caller, or not at all.
- **Applies to:** new functions, methods, exports and `pub` items
- **Checked by:** review
- **Bad:** `db()` and `releaseDb()` shipped with no caller while every handler uses the pool directly
- **Good:** the helper lands in the diff that first calls it
- **Why:** an unused helper looks like the sanctioned path and invites misuse.

### DFS — A struct field that holds the same value at every construction site is removed and its value used directly.
- **Applies to:** structs, records, classes and option objects
- **Checked by:** review
- **Bad:** `behavior: Behavior::AutoKill` at every construction site of `AnomalyRule`
- **Good:** no `behavior` field; the code kills the run where the rule fires
- **Why:** an invariant field poses as configuration and misleads readers.

### ORP — A rename, delete, move or format change leaves no stale reference anywhere in the repository.
- **Applies to:** renames, deletions, moves and format changes of files, symbols, keys, commands and skills
- **Checked by:** review
- **Bad:** `rg old_name src/`, which skips hidden directories and every path outside `src/`
- **Good:** `git grep -n -i -w old_name` from the root with no path filter, repeated for the old basename and path prefix
- **Why:** stale references in tests, SQL, docs and configuration compile cleanly and fail at runtime.

The sweep covers hidden directories and case variants, and separates true targets from same-spelling hits with another meaning.
A directory that a delete or move leaves empty is removed in the same change.

## Grounding and design

### GRD — A claim about an existing surface, in a spec, document or diff, matches its source of truth and cites it by path.
- **Applies to:** specs, architecture docs, scenarios and diffs that state how an existing system works
- **Checked by:** review
- **Bad:** a spec saying platform keys load from server configuration while the schema stores them in a workspace vault
- **Good:** the spec cites the schema file, the handler and the merged spec that locked the decision
- **Why:** an internally consistent but ungrounded framing sends review round after round chasing the same error.

Sources are consulted in this order: playbooks, merged specs, pending specs, architecture docs, then schema, handlers and fixtures.
A conflict between two locked sources goes to the owner before either is overridden.

### PORT — A port to another language carries over observable behaviour and guarantees, never the source language's workarounds.
- **Applies to:** ports and rewrites from one language to another
- **Checked by:** review
- **Bad:** a thread polling a stop flag, carried into Rust line for line
- **Good:** a task cancelled through a `CancellationToken`, with a note naming the guarantee that replaced the flag
- **Why:** a transliteration freezes the old language's debt into the new code.

Conformance tests pin wire bytes, rows, status codes and exit codes, never internal shape.
Each construct kept states why the target language has no better mechanism.
Parity means behavioural parity, never a one-to-one mapping of files, types or threads.

### CODE-SEAM — A new read of a key widens the existing resolver of that key instead of adding a store, statement, trait or seam.
- **Applies to:** new reads of data the code already resolves somewhere
- **Checked by:** review
- **Bad:** a new `ProjectNameStore` and a second `SELECT name FROM projects` for a value `ProjectResolver` already loads
- **Good:** `ProjectResolver` returns the extra field and the new caller uses it
- **Why:** two resolvers of one key drift apart, and each fix lands in only one of them.

## Types and parsing

### TGU — Mutually exclusive variants are an enum with data, never a struct of optional fields.
- **Applies to:** types that model alternatives
- **Checked by:** review
- **Bad:** `struct Trigger { source: Option<String>, schedule: Option<Cron> }`
- **Good:** `enum Trigger { Webhook { source: String }, Schedule(Cron) }`, or a TypeScript union discriminated by `kind`
- **Why:** optional fields make invalid states representable, and a tagged union makes them impossible.

### NTP — External input is parsed into a narrow type at the boundary, never kept as a raw string for a finite set of values.
- **Applies to:** request bodies, configuration, command-line arguments, environment variables and wire payloads
- **Checked by:** review
- **Bad:** `decision: String` compared with `"approve"` deep in the business logic
- **Good:** `decision: Decision` with `enum Decision { Approve, Deny }`, rejected during deserialization
- **Why:** validation deferred past the boundary lets bad values in silently.

### NTE — A callback context carries its concrete type through generics, never through type erasure.
- **Applies to:** callback contexts, hooks and host-supplied values
- **Checked by:** review
- **Bad:** `ctx: &dyn Any` with `ctx.downcast_ref::<PgPool>()`, or `ctx as unknown as Pool` in TypeScript
- **Good:** `fn verify<C: Lookup>(ctx: &C)`, or `function verify<C extends Lookup>(ctx: C)`
- **Why:** an erased type accepts any mismatch at compile time and fails at runtime.

Erasure stays where a uniform collection needs it, such as `Vec<Box<dyn Middleware>>`.

### PSR — A structured format is read with a real parser, never with string search or a regular expression.
- **Applies to:** JSON, YAML, TOML, XML, URLs and similar formats
- **Checked by:** review
- **Bad:** `body.indexOf('"created_at":')` to pull a field out of JSON
- **Good:** `serde_json::from_str::<Event>(body)`, or `JSON.parse` checked against a schema
- **Why:** hand-rolled parsing drops data silently and invites injection.

### RSP — An unsupported pattern is rejected at parse time with a clear error, never matched another way.
- **Applies to:** globs, routes, filters and other user-supplied patterns
- **Checked by:** review
- **Bad:** `api/*/keys` treated as an exact match because mid-path wildcards are unsupported
- **Good:** parsing `api/*/keys` fails with `mid-path wildcards are not supported`
- **Why:** a silent fallthrough hides configuration bugs from operators.

### DRV — Independent values get their own fields, never derived by slicing a sibling field.
- **Applies to:** structs, records and parsed headers
- **Checked by:** review
- **Bad:** `let version = &header[..2];` to read `v0` out of `v0=abc123`
- **Good:** `struct Signature { version: Version, digest: String }`, both set by the parser
- **Why:** a derived slice couples two values invisibly and breaks when either one changes.

### CFG — Variants that differ only in data are configuration values, not enum arms with per-variant code.
- **Applies to:** multi-provider integrations such as webhooks, model providers and identity providers
- **Checked by:** review
- **Bad:** `match provider { Slack => verify_slack(req), Github => verify_github(req) }` with near-identical bodies
- **Good:** `const SLACK: VerifyConfig = VerifyConfig { header: "x-slack-signature", prefix: "v0=" };` passed to one `verify`
- **Why:** a new provider then costs one constant instead of new functions and match arms.

## Errors

### ECL — Errors keep their class, so a timeout, a fatal error and a retryable error are handled differently.
- **Applies to:** network, datastore and process calls, and their callers
- **Checked by:** review
- **Bad:** `Err(_) => return None` around a socket read, so a reset connection busy-loops; or a 503 handled like a 404
- **Good:** a timeout retries with backoff, a 404 stops, and any other error propagates to the caller
- **Why:** one handling for every failure creates busy-loops and misleads callers.

### ESO — A fallible extraction of user-linked data returns its error, never a default value.
- **Applies to:** workspace, user, project and session identifiers read from trusted input
- **Checked by:** review
- **Bad:** `extract_workspace_id(&body).unwrap_or_default()`, or `catch { return ""; }` in TypeScript
- **Good:** `extract_workspace_id(&body)?`, so the failure reaches the caller
- **Why:** a silent default makes the caller act on the wrong record, with no log and no error.

### EMS — An emitted error pairs a registered code with a constant message, never an inline string.
- **Applies to:** HTTP problem responses, command-line errors and error logs
- **Checked by:** review
- **Bad:** `problem(PROJECT_NOT_FOUND, "project is gone")` here and `problem("NOT_FOUND", msg)` in a sibling handler
- **Good:** `problem(PROJECT_NOT_FOUND, PROJECT_NOT_FOUND_MESSAGE)`, with identifiers in structured fields
- **Why:** one structure keeps codes searchable and operator tooling working.

### CODE-ERRREG — Every error code a surface emits is declared once in the repository's error registry, in the same commit as its first use.
- **Applies to:** error codes in source, HTTP responses, command-line output and logs
- **Checked by:** review
- **Bad:** a handler returning the literal `"APP-PROJECT-014"` with no registry entry
- **Good:** `pub const PROJECT_PAUSED: ErrorCode = ErrorCode::declare("APP-PROJECT-014");` in the registry, used by name
- **Why:** an undeclared code cannot be traced from the command line through the daemon to the docs.

Removing a code removes every reference in the same commit, and a declared code with no reference is dead code.

## Resources and concurrency

### OWN — Every resource has exactly one owner and one cleanup path.
- **Applies to:** memory, file handles, connections, locks, subprocesses and temporary files
- **Checked by:** review
- **Bad:** `catch (err) { conn.release(); throw err; } finally { conn.release(); }`
- **Good:** one `finally { conn.release(); }`, or a Rust guard whose `Drop` releases the connection
- **Why:** two cleanup paths release twice on the error path.

Shared ownership is explicit, such as `Arc`, and a borrowed reference never cleans up.

### FLS — Every transport layer is flushed and every query result drained on every exit path.
- **Applies to:** sockets, Transport Layer Security (TLS) streams, buffered writers, database cursors and streamed responses
- **Checked by:** review
- **Bad:** a frame written to a buffered TLS stream, then a return without `flush().await`, so the peer waits forever
- **Good:** `writer.flush().await?` before awaiting the reply, and query results consumed through the wrapper that drains them
- **Why:** a TLS write only encrypts into a buffer, and an undrained result leaves the connection unusable.

### CAS — After a lost compare-and-swap, the loser reads nothing from the slot until a separate ready flag says the winner has finished writing.
- **Applies to:** lock-free code built on atomics
- **Checked by:** review
- **Bad:** `if slot.state.compare_exchange(EMPTY, BUSY, AcqRel, Acquire).is_err() { return slot.read(); }`
- **Good:** the loser waits for `slot.ready.load(Acquire)`, which the winner sets with `store(true, Release)` after writing
- **Why:** the winner may still be writing when the loser reads, so a partial write is visible.

### TIM — Timing relationships between intervals and timeouts are written down and asserted in code.
- **Applies to:** heartbeats, socket timeouts, proxy idle timeouts, leases and retry budgets
- **Checked by:** review
- **Bad:** a 30 s heartbeat behind a 25 s socket timeout, so the proxy drops the connection
- **Good:** `const _: () = assert!(HEARTBEAT_SECS < SOCKET_TIMEOUT_SECS && SOCKET_TIMEOUT_SECS < PROXY_IDLE_SECS);`
- **Why:** an unstated ordering breaks silently when someone tunes one value.

## Untrusted input

### PRI — Outside input reaches a model prompt only after it is validated, typed and length-bounded, and never as instructions.
- **Applies to:** prompts, tool arguments and agent instructions built from user or third-party input
- **Checked by:** review
- **Bad:** `const prompt = "Summarise this ticket: " + ticket.body;`
- **Good:** the body parsed into a typed `Ticket`, bounded in length, and passed as delimited data in its own message
- **Why:** unchecked input lets an outside party steer the agent's decisions and tool calls.

### ESC — A hand-written JSON encoder escapes every control character from U+0000 to U+001F.
- **Applies to:** custom JSON, JSON Lines and event-stream encoders
- **Checked by:** review
- **Bad:** an encoder that escapes only `"` and `\`, so a newline in a value injects a key
- **Good:** `serde_json` or `JSON.stringify`, or a custom encoder escaping all of U+0000 to U+001F as Request for Comments (RFC) 8259 requires
- **Why:** an unescaped control character produces malformed JSON and opens key injection.

### FXS — Security-relevant input is scanned whole or in overlapping chunks, never truncated at a fixed buffer size.
- **Applies to:** injection detectors, content filters, secret scanners and normalisers
- **Checked by:** review
- **Bad:** normalising only the first 64 KiB, so 65 KiB of padding pushes the payload past the scan
- **Good:** overlapping windows across the whole input, or a rejection of input over the limit
- **Why:** an attacker prepends padding larger than the buffer to move the payload out of view.

## Size and naming

### FLL — A source file is at most 350 lines, a function at most 50 and a method at most 70.
- **Applies to:** all source files, tests included; not Markdown, `docs/`, vendored code or generated output
- **Checked by:** review
- **Bad:** a 420-line `handlers.rs`, or a 340-line file with no room left for the tests that cover it
- **Good:** `project_list.rs` split out beside `project.rs`, named for the concern it holds
- **Why:** long files and functions hide their structure and leave no room for tests.

The cap is a ceiling, not a budget: split earlier when two files read better than one.
The first cut moves inline tests and test fakes to a `_test` sibling.

### CODE-MSID — Source and configuration carry no milestone identifier, spec section reference or dimension token.
- **Applies to:** source files, tests, and `*.toml`, `*.yaml` and `*.json` outside `docs/`
- **Checked by:** review
- **Bad:** `// M12_003: retry added for §2.4`, or a test named `T7 rejects an empty name`
- **Good:** `// Retried because the proxy drops idle sockets.`, and a test named `rejects an empty project name`
- **Why:** the code outlives the milestone, and references to its spec rot.

Specs, Pull Request bodies, changelogs and other Markdown may carry them.

## Git and scripted edits

### CODE-GITZ — Code that reads a list of paths from Git asks for NUL-separated output with `-z`.
- **Applies to:** scripts, hooks and tools that parse `git diff`, `git ls-files` or `git status` output
- **Checked by:** review
- **Bad:** `git diff --cached --name-only`, which prints `src/café.ts` as a quoted escape sequence
- **Good:** `git diff --cached --name-only -z`, split on NUL
- **Why:** the default `core.quotePath` quotes unusual bytes, so a classifier silently misses those paths.

### CODE-BRANCH — Code resolves the current branch with `git symbolic-ref --quiet HEAD` and strips its `refs/heads/` prefix.
- **Applies to:** scripts, hooks and gates that name the current branch
- **Checked by:** review
- **Bad:** `git rev-parse --abbrev-ref HEAD`, which prints `HEAD` when detached and `heads/<name>` when a tag shares the name, both with exit 0
- **Good:** `git symbolic-ref --quiet HEAD`, whose non-zero exit means a detached HEAD
- **Why:** a wrong branch name with a zero exit passes every check built on it.

### CODE-BULK — A scripted search or bulk replacement anchors on syntax and asserts its match count, and a zero is confirmed a second way.
- **Applies to:** codemods, scripted substitutions, and searches whose result decides an edit
- **Checked by:** review
- **Bad:** `.replace("sqlx.workspace", …)` with no count, which silently skips `sqlx = { workspace = true }`; or `git grep -E '\bFoo\b'`, whose `\b` is not portable and can match nothing
- **Good:** `re.subn(r"^sqlx\.workspace = true$", …, flags=re.M)` asserting exactly one match, and `git grep -w Foo`
- **Why:** an unanchored pattern edits text it never meant to, and a false zero hides real hits.
