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

`afd_core`, `afd_crypto`, `afd_db`, `afd_dragonfly`, `afd_auth` gained the alias.
`afd_core`, `afd_crypto`, `afd_db`, `afd_dragonfly` had `source()` corrected.

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

**Applies wherever a crate's error COMPOSES a source or CARRIES data.** That is
the test, and it is a property of the error rather than a list of crate names —
the previous wording named crates, which let a crate keep a plain enum by having
been on the list rather than by earning it. If any kind holds a `#[source]`, a
`#[from]`, or a bound field, the crate calls both macros. The hull exists to stop
nine crates hand-writing the same boxed struct, captured backtrace, `[CODE]`
`Display` and self-skipping `source()`, and an error with a cause or a payload
has all four to share.

**A fieldless refusal vocabulary keeps its plain enum**, and this is a
performance rule, not an exemption. `afd_auth::Error` is seven fieldless
variants — `Copy`, `const fn code()`, `const fn detail()`, `pub const ALL` — and
`afd_http`'s guard returns it on every request that fails to authenticate. It has
no source to skip and no backtrace to box, so the hull would add a heap
allocation and a `Backtrace::capture()` to the hottest refusal path in the
product, buy nothing, and cost the exhaustive `ALL` walk that pins each detail
string byte-for-byte against its Zig constant. `afd_sse::Error` is the same shape
for the same reason. A crate on THIS side of the line must say which property put
it there, in its own module note — being a plain enum today is not the argument.

**`afd_state` inherits rather than declares.** It implements `afd_auth`'s
`CredentialDirectory` and `CapabilitySource`, whose signatures mandate
`Unavailable`, so it owns no error to give a hull to. A crate implementing a
foreign trait does not choose the trait's error type.

**The `Result` alias stays hand-written**, in every crate, including those
calling the macro. An alias that only appears after macro expansion is one a
reader cannot see, which is the thing rule 1 exists to prevent.

**A finer-grained type still lifts.** The carve-out below lets a crate keep a
second type where a caller DISCRIMINATES on it — and that type still composes
into the crate's `Error`, through `error_lifts!` or a hand-written `From`, so a
caller that only propagates keeps writing `Result<T>`.

## Conformance, crate by crate

Every crate under `rustd/` is accounted for, and the `Hull` column is the
property test above applied crate by crate rather than a second list to keep in
sync. One row is open: `agentsfleetd` composes and has no hull yet.

| Crate | Owns an `Error` | Hull | Notes |
|---|---|---|---|
| `afd_core` | ✅ | `error_shell!` + `error_lifts!` | `struct Error` + private `ErrorKind`, per M-ERRORS-CANONICAL-STRUCTS. Also DECLARES both macros |
| `afd_crypto`, `afd_db`, `afd_dragonfly`, `afd_bench`, `afd_fleet`, `afd_tenant` | ✅ | `error_shell!` | same shape; each composes through hand-written `From`s rather than the lift macro |
| `afd_admin`, `afd_admission`, `afd_approval`, `afd_billing`, `afd_connector`, `afd_credential`, `afd_cron`, `afd_events`, `afd_fleet_lifecycle`, `afd_fleet_ops`, `afd_fleet_runtime`, `afd_gate`, `afd_identity`, `afd_ingress`, `afd_library`, `afd_observability`, `afd_outbound`, `afd_runner`, `afd_vault` | ✅ | `error_shell!` + `error_lifts!` | the full shape: private `ErrorKind`, one `answer()` table pairing each kind with its code and sentence, raisers in `error/raise.rs`, and a `one_of_each_kind()` sample behind `test-util` |
| `afd_auth` | ✅ | none, by design | seven FIELDLESS variants — `Copy`, `const fn code()`, `const fn detail()`, `pub const ALL` — returned per request by `afd_http`'s guard. Nothing to box and no `source()` to skip; see the hull section above |
| `afd_sse` | ✅ | none, by design | one fieldless variant, same reasoning |
| `afd_state` | ❌ by design | n/a | implements `afd_auth`'s `CredentialDirectory` and `CapabilitySource`, whose signatures mandate `Unavailable`. A crate implementing a foreign trait does not choose the trait's error type. The alias defaults to it and the file says why |
| `agentsfleetd` | two, by necessity | **not yet** | `BootFailure` and `MigrateFailure` — see below for why they cannot merge. Both COMPOSE `afd_db::Error`, so both qualify for the hull by the property test above; this is the one remaining gap. Its `Display` bytes are what an operator reads on a failed boot, and `tests/serve.rs` asserts them by `contains`, not equality, so the `[CODE]` prefix the hull adds would survive |
| `afd_identity` (second type) | ✅ | via the crate hull | `BlankSecret` folded into `Error`; `ClaimUnavailable` and `MetadataUnwritten` kept as their own `Copy` types because callers DISCRIMINATE on them, and composed by `error_lifts!` |
| `afd_api`, `afd_wire`, `afd_webhook`, `afd_api_tenant`, `afd_api_ingress`, `afd_api_operator`, `afd_api_runner` | n/a | n/a | no fallible function. `afd_wire`'s `FailureClass` is a serde field on the wire `Failure` payload, not a Rust error; the plane crates answer with `Refusal`, which is an HTTP response |

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
