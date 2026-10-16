# Rust error standard

Mandatory for every crate under `rustd/`. A new crate follows this on its first
commit; an existing one is not exempt because it predates the rule.

The rules themselves, their examples, the divergence from
`M-ERRORS-CANONICAL-STRUCTS`, and the prior art they rest on are all in
[`.orly/dispatch/write_rust.md`](../.orly/dispatch/write_rust.md). **This file is the local
override:** what `rustd/` generates instead of hand-writing, which crates take
the hull and which do not, and where a crate is deliberately off the rule.

## The four rules, and how this repository meets them

The dispatch page above fires on every `*.rs` edit and carries the rules. This
section shows only the local shape, because the portable rule and the reference
guideline both describe something this repository does not hand-write.

> [!IMPORTANT]
> **`afd_core::error_shell!` is the implementation. Do not hand-write the error
> type.** `M-ERRORS-CANONICAL-STRUCTS` prescribes a situation-specific `struct`
> carrying a `Backtrace`; `.orly/dispatch/write_rust.md` prescribes a `#[from]`-composed
> flat enum behind one alias. Under `rustd/` a crate writes neither by hand: it
> declares a private `ErrorKind`, calls `error_shell!` for the boxed `struct
> Error` with its captured backtrace, its `[CODE] message` `Display` and its
> self-skipping `source()`, and calls `error_lifts!` for the per-source `From`
> impls. A hand-rolled `struct Error` in a new crate is a defect even though both
> upstream texts describe one.

| # | Rule | Met here by | Decided by |
|---|---|---|---|
| 1 | One error type per crate, one `Result` alias beside it | `error_shell!` for the type; the alias is **hand-written**, always | `.orly/audits/rust-error.sh` |
| 2 | Compose with `From`; `?` does the lifting | `error_lifts!`, or a hand-written `From` where a crate composes only a few sources | reviewer |
| 3 | `map_err` only to ADD context the call site alone knows | unchanged — no macro involved | `.orly/audits/rust-error.sh`, which catches a `map_err` that stringifies its own cause |
| 4 | `source()` returns what caused you, never yourself | `error_shell!` generates the self-skipping `source()` | reviewer |

**The alias is the one part that stays hand-written**, in every crate, including
those calling the macro. An alias that only appears after macro expansion is one
a reader cannot see, which is the thing rule 1 exists to prevent.

**Not every crate takes the hull**, and the test is a property of the error
rather than a list of names — see §"The shared hull" below for which crates are
on each side and why a fieldless refusal vocabulary stays a plain enum.

**One deliberate exception, and it is security not style.** `afd_crypto`'s
`ErrorKind::OpenFailed` declines to wrap the Authenticated Encryption with
Associated Data (AEAD) library's reason: a wrong key, a tampered tag and
mismatched associated data all answer `UZ-INTERNAL-003`. Telling a caller which
one it was would hand it a decryption oracle.

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
product, buy nothing, and cost the exhaustive `ALL` walk that pins each
client-visible detail string byte-for-byte
(`rustd/crates/afd_auth/tests/auth_error_taxonomy.rs`). `afd_sse::Error` is the same shape
for the same reason. A crate on THIS side of the line must say which property put
it there, in its own module note — being a plain enum today is not the argument.

**`afd_state` inherits rather than declares.** It implements `afd_auth`'s
`CredentialDirectory` and `CapabilitySource`, whose signatures mandate
`Unavailable`, so it owns no error to give a hull to. A crate implementing a
foreign trait does not choose the trait's error type.

**A finer-grained type still lifts.** The carve-out below lets a crate keep a
second type where a caller DISCRIMINATES on it — and that type still composes
into the crate's `Error`, through `error_lifts!` or a hand-written `From`, so a
caller that only propagates keeps writing `Result<T>`.

## Conformance, crate by crate

Every crate under `rustd/` is accounted for, and the `Hull` column is the
property test above applied crate by crate rather than a second list to keep in
sync. No row is open: `agentsfleetd`'s two types stay off the hull for the
property their row names.

| Crate | Owns an `Error` | Hull | Notes |
|---|---|---|---|
| `afd_core` | ✅ | `error_shell!` + `error_lifts!` | `struct Error` + private `ErrorKind`, per M-ERRORS-CANONICAL-STRUCTS. Also DECLARES both macros |
| `afd_crypto`, `afd_dragonfly`, `afd_mail`, `afd_tenant` | ✅ | `error_shell!` + `error_lifts!` | same shape; each foreign source lifts through the macro |
| `afd_db` | ✅ | `error_shell!` | same shape; composes no foreign error through `From` |
| `afd_fleet` | ✅ | `error_shell!` + `error_lifts!` | same shape; its sources lift through the macro in `error/lift.rs` |
| `afd_bench` | ✅ | `error_shell!` + `error_lifts!` | private `ErrorKind` lifting its fourteen sources; reuses existing registry codes rather than minting a `BENCH_*` family, since a bench refusal is read by the developer running the make target, never rendered to a tenant |
| `afd_admin`, `afd_admission`, `afd_approval`, `afd_billing`, `afd_connector`, `afd_credential`, `afd_cron`, `afd_events`, `afd_fleet_lifecycle`, `afd_fleet_ops`, `afd_fleet_runtime`, `afd_gate`, `afd_identity`, `afd_ingress`, `afd_library`, `afd_observability`, `afd_outbound`, `afd_runner`, `afd_vault` | ✅ | `error_shell!` + `error_lifts!` | same shape: private `ErrorKind`, a `code()` naming an existing registry code, and raiser functions beside it. Most also keep raisers in `error/raise.rs`, an `answer()` table pairing each kind with its code and sentence, and a `one_of_each_kind()` sample behind `test-util`; not every crate has all three |
| `afd_memory` | ✅ | `error_shell!` + `error_lifts!` | composes `afd_db`, `sqlx`, `afd_crypto` and `afd_core` errors and carries a context, a detail and a store name, so it takes the hull |
| `afd_otlp` | ✅ | `error_shell!` + `error_lifts!` | composes the exporter's build errors. Its second type, `Refused` (a knob and the sentence it accepts), is DISCRIMINATED on: the daemon's preflight reports each refused knob beside every other fault, so it needs the knob as data, and `Refused` lifts into `Error` as the `Knob` kind |
| `afd_observability` (second type) | ✅ | via the crate hull | `ParsePolicy` is the `FromStr::Err` a trait mandates for `Policy`; no fallible function answers with it, and serde flattens it at its one call site. It carries the number parser's error as a `#[source]` rather than a string |
| `afr_agent`, `afr_executor`, `afr_sandbox`, `afr_supervisor`, `afr_secrets`, `afr_providers`, `afr_telemetry` | ✅ | `error_shell!` + `error_lifts!` | the Rust runner's crates: private `ErrorKind`, `code()` answering with EXISTING registry codes — a runner failure is read by an operator on the host's journal, never by a client, so it mints no `UZ-RUN-*` code that `public/openapi.json` would publish; the log line's `event` says which failure it was, as `afd_bench` does |
| `afr_egress` | ✅ | `error_shell!` + `error_lifts!` | a refused request carries its host, method or credential name, so it takes the hull. Every refusal goes back to the model as an error: the model reads `detail()`, never the `[CODE]` rendering, and a tool picks the model's code from `refusal()`, a fieldless `Copy` `Refusal` it discriminates on (`afr_tools/src/egress/mod.rs`). A mint the daemon refused keeps the daemon's code |
| `afr_memory` | ✅ | `error_shell!` + `error_lifts!` | composes garde's report and the secret matcher's build error; each source arises at one call site, so each lift carries one meaning |
| `afr_tools` | ✅ | `error_shell!` | no kind wraps a cause, so nothing lifts. `ToolErrorCode` and `Unanswered` are fieldless `Copy` enums the agent loop DISCRIMINATES on to choose what the model is told, and stay plain |
| `afr_providers` (second type) | ✅ | via the crate hull | `transport::Oversize` is found by walking the error chain, the one way a caller tells a response over its cap from any other transport failure; it composes into `Error` |
| `afr_egress` (second type) | ✅ | via the crate hull | `BlockedAddress` is found by walking a resolver's error chain, where the guard that refused a private address is the only thing that knows it did; reqwest owns the error between them, so it cannot compose by `From` |
| `afd_auth` | ✅ | none, by design | seven FIELDLESS variants — `Copy`, `const fn code()`, `const fn detail()`, `pub const ALL` — returned per request by `afd_http`'s guard. Nothing to box and no `source()` to skip; see the hull section above |
| `afd_sse` | ✅ | none, by design | one fieldless variant, same reasoning |
| `afd_validate` | ❌ by design | n/a | owns no error: a garde rule answers with garde's own error, and `Limit::parse` with `LimitBreak`, a fieldless `Copy` refusal whose two variants the routes map to their two sentences |
| `agentsfleet_runner` | ❌ by design | n/a | the terminal binary: every failure ends at `OrExit::or_exit` or `io_failed`, which logs it under `run_failed` with its registry code and turns it into an `ExitCode`. Nothing receives its failures, so its signatures are `Result<T, ExitCode>` and it has no `Result` alias |
| `afd_state` | ❌ by design | n/a | re-exports `afd_auth::error::Unavailable`, the error its foreign traits mandate (see §"The shared hull"); its `Result` alias defaults to it and `src/error.rs` says why |
| `agentsfleetd` | two, by necessity | none, by design | `BootFailure` and `MigrateFailure`, both in `src/error.rs` beside the preflight `Fault` and `Refusal` that lift into `BootFailure` — see below for why they cannot merge. Both compose `afd_db::Error`, which the property test would put on the hull, and the property that keeps them off it is that their `Display` IS the output: `fatal` prints it as the boot sentence and walks `source()` beneath it, and `serve` sends it as the analytics event's `reason` beside the `error_code` field. The hull's `[CODE]` prefix would repeat that code in both, and its captured backtrace would print a second, unfiltered time beside `fatal`'s own frames and reach the analytics sink under `RUST_BACKTRACE` |
| `afd_api_wire` | n/a | n/a | no fallible function: request and response shapes, refused by garde on the type |
| `afd_identity` (second type) | ✅ | via the crate hull | `BlankSecret` folded into `Error`; `ClaimUnavailable` and `MetadataUnwritten` kept as their own `Copy` types because callers DISCRIMINATE on them, and composed by `error_lifts!` |
| `afd_api` | n/a | n/a | no fallible function: it assembles the router and server from the plane crates |
| `afd_http`, `afd_api_tenant`, `afd_api_ingress`, `afd_api_operator`, `afd_api_runner` | ❌ by design | n/a | a handler's failure is an HTTP response. Handlers answer `Result<Response, Refusal>`, where `afd_http`'s `Refusal` boxes the rendered `Response` (`afd_http/src/handler/refusal/mod.rs`); request parsers answer a fieldless refusal such as `BrokenEscape` or a static sentence the handler renders |
| `afd_wire` | ❌ by design | n/a | `TraceRejection` and `DetailRejection` are fieldless `Copy` refusals naming the bound a tool trace or tool record broke; the daemon logs the reason and drops the item (`afd_fleet/src/lease/tool_detail/mod.rs`). `FailureClass` is the `failure_reason` field on the wire `ReportRequest`, not a Rust error |
| `afd_webhook` | ❌ by design | n/a | a delivery that fails to verify is a `Verdict`, and its fieldless `Refusal` names one of the three documented reasons; nothing on this side failed, so it is not an error (`afd_webhook/src/verdict.rs`) |

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
