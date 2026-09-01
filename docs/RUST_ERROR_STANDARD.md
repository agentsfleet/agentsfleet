# Rust error standard

Mandatory for every crate under `rustd/`. A new crate follows this on its first
commit; an existing one is not exempt because it predates the rule.

The shape is not invented here. It is what `core_api` has run in production on
for years (10 crates, each with `pub type Result<T>` and a flat `Error`), what
bun uses (`thiserror`, `#[from]` composition, `pub type Result<T, E = Error>`),
and what habitat uses (one payload-carrying `enum Error`, one `Result` alias).

## The four rules

### 1. One error type per crate, one `Result` alias beside it

```rust
pub type Result<T, E = Error> = core::result::Result<T, E>;
```

In `src/error.rs`, next to the type it defaults to. Every fallible function in
the crate returns `Result<T>`. The default parameter is what lets the few
functions answering with a foreign error keep the same spelling —
`Result<T, VerifyError>` — instead of reaching for `std::result::Result`.

A reader must never have to check *which* error a signature returns to know it
is this crate's.

### 2. Compose with `From`; `?` does the lifting

A conversion that adds nothing is a `#[from]`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum BootFailure {
    #[error(transparent)]
    Environment(#[from] Refusal),
    #[error("agentsfleetd cannot boot: the API database would not answer")]
    Database(#[from] afd_db::Error),
}
```

Then the call site is `Db::connect(cfg).await?` — no `map_err`, no lost detail.

### 3. `map_err` only to ADD context the call site alone knows

Keep it when the error gains a fact the source could not carry:

```rust
// CORRECT — the role and the budget are the caller's knowledge.
.map_err(|source| classify_acquire(role.tag(), acquire_timeout.as_millis(), source))?
```

Delete it when it only relabels. And never do this:

```rust
// WRONG — a lossy conversion wearing a conversion's clothes.
.map_err(|error| BootFailure::Database(error.to_string()))?
```

`to_string()` on the way *into* an error type destroys the `source()` chain.
It compiles, it reads fine, and it silently defeats every chain walker
downstream — including this daemon's own fatal renderer.

### 4. `source()` returns what caused you, never yourself

An error whose `Display` already renders its kind must not also return that
kind as its source. A chain walker then prints the same sentence twice before
reaching anything new.

```rust
// WRONG — `Display` is "[code] {kind}", so the kind is already printed.
fn source(&self) -> Option<&(dyn Error + 'static)> { Some(&self.kind) }

// RIGHT — skip ourselves, hand back what the kind wraps.
fn source(&self) -> Option<&(dyn Error + 'static)> {
    std::error::Error::source(&self.kind)
}
```

**Not every error has a cause, and that is not a defect.** A variant holding
another error (`Unreachable { source: sqlx::Error }`) has one; a variant holding
only data (`MissingDatabaseUrl { knob }`) does not — nothing *caused* an unset
variable. A test asserting `source().is_some()` for every variant is wrong and
forces authors to invent causes. Assert the real invariant instead: *where there
is a source, it is not a repeat of our own message.*

One deliberate exception, and it is security not style: `afd_crypto`'s
`EnvelopeOpen` declines to wrap the AEAD library's reason. Telling a caller
"bad tag" versus "bad nonce" is the beginning of a padding oracle.

## What changed under this standard (M176)

| Measure | Before | After |
|---|---|---|
| Crates with a `Result` alias | 0 | 5 |
| Explicit `Result<T, Error>` spellings | 55 | 0 |
| `From` / `#[from]` conversions | 2 | 7 |
| `map_err` in `agentsfleetd` | 5 | 0 |
| `map_err` elsewhere | 54 | 54 — all context-adding, correct |
| Crates whose `source()` returned themselves | 4 | 0 |

`afd_core`, `afd_crypto`, `afd_db`, `afd_redis`, `afd_auth` gained the alias.
`afd_core`, `afd_crypto`, `afd_db`, `afd_redis` had `source()` corrected.

The corrected chain, from a real boot against a stopped Postgres:

```text
BEFORE                              AFTER
✗ cannot boot: <one string>         ✗ cannot boot: the API database
                                      would not answer
(chain ends — to_string() ate it)     caused by: [UZ-INTERNAL-001] the api
                                        datastore is unreachable
                                        caused by: error communicating with
                                          database: Connection refused
                                          caused by: Connection refused
                                            (os error 61)
```

## The shared hull: `error_shell!` and `error_lifts!`

Rules 1, 2 and 4 describe scaffolding every crate-level error repeats — the
boxed `struct Error` with its captured backtrace, the `From<ErrorKind>` that is
the one place a kind becomes an error, the `Display` rendering `[CODE]
message`, and the `source()` that skips the kind. None of it depends on what
went wrong, so `afd_core::error_shell!` generates it and
`afd_core::error_lifts!` generates the per-source `From` impls rule 2 asks for.

**Applies to a crate whose error is a boxed struct over a private kind.** A
crate whose `Error` is a plain `thiserror` enum (`afd_auth`, `afd_sse`,
`afd_identity`, and nine others) has no hull to share and calls neither macro —
that is conformance, not a gap.

**The `Result` alias stays hand-written**, in every crate, including those
calling the macro. An alias that only appears after macro expansion is one a
reader cannot see, which is the thing rule 1 exists to prevent.

**A finer-grained type still lifts.** The carve-out below lets a crate keep a
second type where a caller DISCRIMINATES on it — and that type still composes
into the crate's `Error`, through `error_lifts!` or a hand-written `From`, so a
caller that only propagates keeps writing `Result<T>`.

## Conformance, crate by crate

Every crate under `rustd/` is accounted for. The three items this section used
to list as open are closed.

| Crate | `src/error.rs` | Owns an `Error` | Notes |
|---|---|---|---|
| `afd_core` | ✅ | ✅ | `struct Error` + private `ErrorKind`, per M-ERRORS-CANONICAL-STRUCTS |
| `afd_crypto` | ✅ | ✅ | same shape |
| `afd_db` | ✅ | ✅ | same shape |
| `afd_redis` | ✅ | ✅ | same shape |
| `afd_auth` | ✅ | ✅ | was `AuthError`; renamed to `Error`, so `afd_auth::Error` no longer stutters. `VerifyError` and `Unavailable` live beside it and stay distinct — see below |
| `afd_identity` | ✅ | ✅ | `BlankSecret` folded into `Error`; `ClaimUnavailable` kept and composed by `#[from]` |
| `afd_state` | ✅ | ❌ by design | implements `afd_auth`'s `CredentialDirectory` and `CapabilitySource`, whose signatures mandate `Unavailable`. A crate implementing a foreign trait does not choose the trait's error type. The alias defaults to it and the file says why |
| `agentsfleetd` | ✅ | two, by necessity | `BootFailure` and `MigrateFailure` — see below |
| `afd_api` | n/a | n/a | no fallible function |
| `afd_observability` | n/a | n/a | no fallible function |
| `afd_wire` | n/a | n/a | no fallible function. `FailureClass` is a serde field on the wire `Failure` payload, not a Rust error |
| `afd_webhook` | n/a | n/a | no fallible function |
| `afd_api_tenant`, `afd_api_ingress`, `afd_api_operator`, `afd_api_runner` | n/a | n/a | no fallible function. The plane crates the fleet decomposition split out of `afd_api`; each answers with `Refusal`, which is an HTTP response and not an error type |

### Where rule 1 is deliberately not met, and why

**`agentsfleetd` has two error types.** They cannot be merged. Both compose
`afd_db::Error` by `#[from]` — boot's when the API pool will not open,
migrate's when the schema will not apply — and one enum cannot carry two
variants deriving `From<afd_db::Error>`, because that is two `From` impls for
one pair of types. Collapsing them into a single variant would be worse than
the duplication: "the API database would not answer" and "the schema was not
applied" are different incidents with different fixes, and `serve` and
`migrate` are different processes that never run at once. The crate therefore
carries no `Result` alias either: it would have to default to one of the two,
and a reader seeing the short spelling would have to check which — the exact
thing rule 1 exists to prevent.

**A crate may keep a second, finer-grained type where a caller
DISCRIMINATES on it.** `afd_auth::VerifyError` is finer than what a client is
told, on purpose, and `afd_identity::ClaimUnavailable::UnknownSubject` is
deliberately not an outage — the caller matches on it and answers with the
empty capability set. Both compose into their crate's `Error` by `From`, so a
caller that only propagates still writes `Result<T>`. A type that nothing
discriminates on has not earned this: `afd_identity`'s `BlankSecret` was a unit
struct exactly one function returned and nothing matched on, and it is a
variant of `Error` now.
