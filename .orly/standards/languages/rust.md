# Rust

This standard covers Rust crates: the error type, the fallible path, panics, ownership, `unsafe`, features and tests.
A coding agent reads it before editing a `*.rs` or `Cargo.toml` file, and a reviewer judges a diff against it.

## Reference reading

### RS-REF — A Rust design follows a named reference implementation, and its review cites the Pragmatic Rust Guidelines it applied or diverged from.
- **Applies to:** `**/*.rs`, at design and at review
- **Checked by:** review
- **Bad:** a new error module designed from memory, and a review that reads "looks idiomatic"
- **Good:** the matching module in the repository's own Rust code read first, and a review that says "applies `M-FROM-ERROR`; diverges from `M-APP-ERROR` per ERR-RS"
- **Why:** a pattern already shipping in a trusted codebase is evidence, and a named divergence is a decision rather than drift.

References: Exonum for state and API boundaries, arrow-rs for typed data and workspace layout, and the repository's daemon crate for errors and commands.
Builder's `ConfigFile` is prior art for composable, cause-preserving file loading, but not for its global environment mutation or logger.
The [Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/agents/all.txt) are read by section, and local convention wins a conflict that the review names.

## Errors

### ERR-RS — Each crate has one error type with a `Result` alias beside it, and every fallible function in the crate returns that alias.
- **Applies to:** `**/*.rs`
- **Checked by:** review
- **Bad:** `fn load(path: &Path) -> std::result::Result<Config, Box<dyn std::error::Error>>`
- **Good:** `pub type Result<T, E = Error> = core::result::Result<T, E>;` beside the crate's `Error`, and `fn load(path: &Path) -> Result<Config>`
- **Why:** a reader never has to check which error a signature returns to know it is this crate's.

The default parameter lets a function that answers with a foreign error keep the spelling, as `Result<T, VerifyError>`.
This diverges from `M-APP-ERROR` and `M-ERRORS-CANONICAL-STRUCTS`: no `anyhow` or `eyre` catch-all, because a catch-all erases the variant a caller matches on.
A crate with two unrelated fallible surfaces may carry a second error type, with its own alias, under these same rules.

### RS-FROM — Errors compose through `From` impls, so `?` lifts a dependency's error with no conversion at the call site.
- **Applies to:** `**/*.rs`
- **Checked by:** review
- **Bad:** `load(path).map_err(BootFailure::Database)?`, repeated at every call site
- **Good:** `Database(#[from] db::Error)` on the error enum, then `load(path)?`
- **Why:** one `From` impl replaces a conversion at every call site, and each hand-written conversion is a chance to drop the cause.

A generated `From`, such as one from an `error_lifts!` macro, counts the same as `#[from]`.

### RS-MAPERR — A `map_err` only adds context the call site alone knows, and never turns the cause into a string.
- **Applies to:** `**/*.rs`
- **Checked by:** review
- **Bad:** `.map_err(|error| Mine::Database(error.to_string()))?`
- **Good:** `.map_err(|source| Mine::Budget { budget, source })?`, or a plain `?` through `From` when there is nothing to add
- **Why:** `to_string()` on the way into an error type cuts the `source()` chain, and every chain walker downstream goes blind.

Clippy's `map_err_ignore` catches the `|_|` form, but the `to_string()` form needs review.

### RS-SOURCE — An error's `source()` returns what caused it, never its own kind.
- **Applies to:** `**/*.rs`
- **Checked by:** review
- **Bad:** `fn source(&self) -> Option<&(dyn Error + 'static)> { Some(&self.kind) }`, while `Display` already prints the kind
- **Good:** `fn source(&self) -> Option<&(dyn Error + 'static)> { std::error::Error::source(&self.kind) }`
- **Why:** a chain walker would print the same sentence twice before reaching anything new.

### RS-NOCAUSE — A test asserts that a present `source()` differs from the error's own message, and never demands a source from every variant.
- **Applies to:** `**/*.rs`
- **Checked by:** review
- **Bad:** `assert!(error.source().is_some())` for every variant, including `MissingUrl { knob }`
- **Good:** `if let Some(source) = error.source() { assert_ne!(source.to_string(), error.to_string()); }`
- **Why:** a variant that holds only data has no cause, and a test that demands one makes authors invent causes.

## The fallible path

### FN-RS — The fallible path is a pipeline of `?` and combinators, and a `match` appears only where its arms carry different logic.
- **Applies to:** `**/*.rs`
- **Checked by:** review
- **Bad:** `match load(path) { Ok(raw) => match parse(&raw) { Ok(c) => c, Err(e) => return Err(e.into()) }, Err(e) => return Err(e.into()) }`
- **Good:** `let config = parse(&load(path)?)?;`
- **Why:** each hand-plumbed failure arm is another place the error can be swallowed.

`map`, `and_then`, `ok_or_else` and `unwrap_or_else` transform success and pass failure through in one line.

### RS-SENTINEL — A fallible function returns `Result`, an absent value is an `Option`, and no sentinel stands in for either.
- **Applies to:** `**/*.rs`
- **Checked by:** review
- **Bad:** `fn find(id: &str) -> i64` returning `-1` when missing, or an error logged and swallowed
- **Good:** `fn find(id: &ProjectId) -> Result<Option<Project>>`
- **Why:** a caller cannot handle a failure that the signature does not show.

A `bool` plus an out-parameter, an empty string and an all-zero identifier are sentinels too.

### RS-NEWTYPE — A checked value is a newtype whose constructor returns `Result`, and interior code that receives it never re-checks.
- **Applies to:** `**/*.rs`
- **Checked by:** review
- **Bad:** `fn charge(budget: i64)` that re-checks `budget > 0`, as do the three functions it calls
- **Good:** `pub struct Budget(NonZeroU64);` built by `Budget::new(raw) -> Result<Budget>` at the boundary, then `fn charge(budget: Budget)`
- **Why:** a type that cannot hold an invalid value turns every defensive re-check into dead code.

The constructor follows `M-STRONG-TYPES-GUARD`, and `M-STRONG-TYPES` picks the type family.

### RS-BIND — A value is bound once from an `if` or `match` expression, never from a mutable default that later branches overwrite.
- **Applies to:** `**/*.rs`
- **Checked by:** review
- **Bad:** `let mut limit = DEFAULT_LIMIT; if paid { limit = PAID_LIMIT; }`
- **Good:** `let limit = if paid { PAID_LIMIT } else { DEFAULT_LIMIT };`
- **Why:** a `mut` that only simulates an expression adds a bug surface and buys nothing.

### RS-UNWRAP — `unwrap()` and `expect()` appear only in tests or behind an invariant already proved, and the `expect` message states the proof.
- **Applies to:** `**/*.rs`
- **Checked by:** linter: clippy unwrap_used; clippy expect_used
- **Bad:** `let guard = state.lock().unwrap();` in production code
- **Good:** `state.lock().expect("mutex never poisoned: no thread panics while holding it")`, under `#[expect(clippy::expect_used, reason = "…")]`
- **Why:** an `unwrap()` with no written invariant is a `Result` nobody designed, and it aborts on the input nobody tested.

`M-PANIC-ON-BUG` decides when a panic is right, and `M-PANIC-MESSAGE` requires the message to help.

## Ownership and unsafe code

### RS-BORROW — A signature shows ownership: a function that only reads a value borrows it, and one that keeps or releases it takes it by value.
- **Applies to:** `**/*.rs`
- **Checked by:** linter: clippy needless_pass_by_value
- **Bad:** `fn render(project: Project) -> String` that only reads `project`
- **Good:** `fn render(project: &Project) -> String`
- **Why:** the caller keeps its value, and the signature alone tells which calls consume a resource.

### RS-UNSAFE — An `unsafe` block is no larger than the operation that needs it and carries a `// SAFETY:` comment naming the invariant it relies on.
- **Applies to:** `**/*.rs`
- **Checked by:** linter: clippy undocumented_unsafe_blocks
- **Bad:** `unsafe { prepare(&mut buf); *buf.get_unchecked(index) }`, with no comment
- **Good:** `// SAFETY: index < buf.len(), checked on the line above.` then `unsafe { *buf.get_unchecked(index) }`
- **Why:** the invariant is the whole proof of soundness, and a reviewer cannot check a proof nobody wrote.

### RS-ENV — Library code reads the process environment through an injected `EnvSource`, and tests inject a `MapEnv` instead of mutating the process.
- **Applies to:** `**/*.rs`
- **Checked by:** linter: clippy disallowed_methods
- **Bad:** `std::env::var("DATABASE_URL")` in a crate, or `unsafe { std::env::set_var("DATABASE_URL", url) }` in a test
- **Good:** `fn resolve<E: EnvSource + ?Sized>(env: &E)` reading `optional(env, URL_KNOB)`, with `MapEnv::from_pairs(…)` passed in tests
- **Why:** an injected source shows every knob in a signature, and edition 2024 marks `set_var` unsafe because other threads may read the environment.

## Features and tests

### RS-COMBOS — Every feature combination the repository builds also compiles, lints and tests, not only the `--all-features` one.
- **Applies to:** `**/*.rs`, `**/Cargo.toml`
- **Checked by:** review
- **Bad:** lanes that run only `--all-features`, while a binary ships with default features
- **Good:** a default-features `cargo check --workspace` beside the `--all-features` lint and test lanes
- **Why:** code that compiles only under unified features ships broken, and a gated item can hide dead code that one combination exposes.

### RS-FEATURE — A crate whose tests use a dependency's `test-util` items forwards that feature from its own `test-util`.
- **Applies to:** `**/Cargo.toml`
- **Checked by:** review
- **Bad:** `test-util = []`, while the crate's tests call `EventDetailRow::fixture` from `app_events`
- **Good:** `test-util = ["app_events/test-util"]`
- **Why:** workspace feature unification hides the gap, so the crate fails to build alone and its gated tests may never run.

### RS-TESTCONST — A `pub(crate)` constant reaches integration tests as a `pub const` binding in a `test-util`-gated module, never through `pub use` or `#[expect(unreachable_pub)]`.
- **Applies to:** `**/*.rs`
- **Checked by:** review
- **Bad:** `pub use crate::hub::NODE_REPAIR_WINDOW;`
- **Good:** `pub const NODE_REPAIR_WINDOW: Duration = crate::hub::NODE_REPAIR_WINDOW;` in a `#[cfg(feature = "test-util")]` module
- **Why:** `pub use` of a crate-private item fails with E0364, and the `expect` goes unfulfilled under `--all-features`, so only the binding stays clean.

A rename of the original then fails to compile at the binding instead of drifting.

### RS-RACE — Concurrent code has a deterministic contention test, not only a single-task happy path.
- **Applies to:** `**/*.rs`
- **Checked by:** review
- **Bad:** `#[tokio::test] async fn claims_lease()` that claims once, alone
- **Good:** 16 claimers released together through a `tokio::sync::Barrier`, with an assertion that exactly one wins
- **Why:** a race appears only under contention, so a single-task test passes on the broken code.
