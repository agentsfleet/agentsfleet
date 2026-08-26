# Handoff — M177_001 runner control plane · §2 code-complete and UNCOMMITTED

Ephemeral. Delete at CHORE(close). Replaces the prior handoff.

## Indy's standing instruction, as of this handoff

**Build code through §6 first. Only `cargo clippy` between sections — no test
runs, no `fmt` sweep — then run all unit tests and `fmt` ONCE at the end.**

That is why the tree below is dirty and why the numbers in Tests are a
snapshot rather than a claim about the current tree.

## Scope / Status

Porting `/v1/runners` to `agentsfleetd-rs`. Spec
`docs/v2/active/M177_001_P0_API_RUNNER_CONTROL_PLANE_PARITY.md`, IN_PROGRESS.

- ✅ §1, ✅ §5
- 🟡 **§2 is CODE-COMPLETE and UNCOMMITTED.** It compiled, passed clippy across
  all targets, and passed the full workspace run — **829 passed, 0 failed, 84
  ignored** — immediately before this handoff. Nothing has changed since.
- 🚩 §2 issue-time run debit — NOT BUILT, by Indy's decision. See Decisions.
- ⏳ §3 report — **next**
- ⏳ §4 activity/memory/bundles/mint, §6 sweepers, §7 harness

**No blockers.**

## Working Tree — DIRTY, and every path is wanted

**37 commits ahead of `origin/main`, none pushed. §2's work is NOT among them.**

17 modified, 8 untracked. Nothing here is stray; commit it as a set:

```
NEW  afd_fleet/src/lease/pull.rs          the verb: claim → gates → money → answer
NEW  afd_fleet/src/lease/deliver.rs       the delivery half: assemble → issue
NEW  afd_fleet/src/lease/answer.rs        rendering, with no decisions in it
NEW  afd_fleet/src/gate/grants.rs         integration-grant + write-gate reads
NEW  afd_fleet/src/sql/grant.rs           the grant statements
NEW  afd_fleet_runtime/src/config/binding_match.rs   recorded-binding matcher
NEW  afd_api/src/handler/runner/lease.rs  the HTTP verb, thin
NEW  afd_fleet/tests/integration_gate_grants.rs      WRITTEN, NEVER RUN
```

The three `lease/*.rs` files and `gate/grants.rs` exist as separate files
because of the S6 350-line cap, not by preference — `pull.rs` reached 453 and
`gate/store.rs` 374.

## Branch / PR

- `feat/m177-runner-control-plane-parity` (GitHub forge)
- **PR: none. CI has never run this branch.**
- **M183_001 rides in this PR** — `docs/v2/pending/M183_001_P1_API_ZIG_WORKAROUND_TRANSLITERATIONS.md`,
  committed at `82c54a504`, already inside the `origin/main..HEAD` range. It is
  another agent's spec, it is expected, and the PR body must describe both
  streams. Do not treat it as stray.

## Running Processes

No tmux server. Daemon detached, pid **68828**, port **3000**:

```bash
kill 68828
curl -s localhost:3000/readyz          # {"ready":true,"database":true,"queue":true}
tail -f .tmp/rustd-serve.log
source .tmp/rustd-env.sh               # before serve or migrate
AGENTSFLEET_LOG_LEVEL=debug ./rustd/target/debug/agentsfleetd serve
```

**That daemon predates §2** — it is running the binary from before the lease
verb was mounted. Rebuild before believing anything it says about `/leases`.

Compose up: postgres **28979** · redis **28980** · qstash **28981**.

**⚠ `.tmp/rustd-env.sh` is NOT a deployment template.** It points
`DATABASE_URL_MIGRATOR` at the same URL as `DATABASE_URL_API`, collapsing a
privilege split Postgres enforces (`schema/110`).

## Routes served

**`mounted=6 tabled=81`** — the two probes, the runner's self read and
heartbeat, enrolment, and now **`POST /v1/runners/me/leases`**.

`tests/router.rs::test_only_the_ported_routes_are_mounted` pins this. It FAILED
correctly when the lease arm landed (it asserts the route 404s) and was updated
to expect six. That test is the tripwire for an unfinished surface going live.

## Tests / Checks

Snapshot from immediately before this handoff, on the current tree:

| Gate | Result |
|---|---|
| `cargo clippy --workspace --all-features --all-targets` | ✅ clean |
| `cargo test --workspace --all-features` | ✅ **829 passed, 0 failed, 84 ignored** |
| `make harness-verify` | ⏳ not run since §2 landed — **stage first, it is staged-scope** |
| `cargo fmt --all` | ⏳ deferred to the end per the standing instruction |
| `make test-unit-all`, `make test-integration-rustd` | ⏳ deferred to the end |

`cargo test --workspace` is the RUST half only — not `make test-unit-all`,
which also carries the TypeScript coverage gates.

**`integration_gate_grants.rs` has never been executed.** One suite, three
tests, needs live Postgres:

```bash
TEST_DATABASE_URL="postgres://agentsfleet:agentsfleet@localhost:28979/agentsfleetdb?sslmode=disable" \
TEST_REDIS_URL="rediss://:agentsfleet@localhost:28980" \
TEST_REDIS_CA_CERT="$PWD/.tmp/redis-ca.crt" \
cargo test -p afd_fleet --all-features --test <suite> -- --ignored
```

## Next Steps

1. **§3 report** — `claimReport()` in one atomic statement: fencing-token
   verification, lease flip, telemetry dedup via the UNIQUE
   `(event_id, charge_type)` ledger rows (writes 7–12). Stale writers get
   `UZ-RUN-005`. Renew extends to
   `min(now + LEASE_TTL_MS, created_at + MAX_RUNTIME_MS)` and re-checks
   coverage. Mount `RunnerRoute::{Report, Renew}` the way `Lease` now is.
2. **§4** activity, memory hydrate/capture, bundles, credentials mint.
3. **§6** sweepers and runner metrics.
4. THEN the full sweep: `cargo fmt --all`, `make test-unit-all`,
   `make test-integration-rustd`, `make harness-verify` staged.
5. Then CHORE(close) and `gh pr create`.

**Wire types for §3/§4 are likely already present and fixture-pinned** — check
`afd_wire` and `afd_wire/tests/roundtrip.rs` against
`samples/fixtures/wire-v2/` before writing any.

## §2 design decisions a reviewer will ask about

- **The verb answers SERIALIZED BYTES, not a struct.** `ExecutionPolicy`
  borrows from the config, the resolved provider and the declared credentials
  — every field a `Cow`, which is what keeps the payload copy-free. A value
  borrowing four locals cannot be returned; deep-owning copies exactly what the
  borrows exist to avoid, and assembling twice puts a second opinion about what
  a run may do in the one place that must have one. So assembly, payload and
  serialization all happen inside the borrow.
- **`Leasing` is an associated type on `Services`**, like `Auth`. The concrete
  plane holds a Redis connection opened by CONNECTING, so the router suites
  cannot construct one — they get a `NoWork` stub. That keeps those suites
  about guards and scopes, where they belong.
- **Two NEW failure labels** — `event_type_unsupported` and
  `binding_unenforceable`, in `sql::event::label`. The first cut reused
  `tenant_resolve_failed` and `secret_missing`; both were lies that would send
  an operator to look at billing or the vault. They have no Zig counterpart and
  say so in their doc comments.
- **The connector registry is a `Plane` FIELD**, not a handler argument. Which
  third parties exist is a composition-root fact.
- **`Fence::as_u64` saturates to ZERO** for a negative. `unsigned_abs` would
  turn `-1` into `1` — a token another holder may legitimately hold, which is a
  plausible wrong answer rather than an obvious one.
- **One duplicate read, accepted.** The verb calls `Accounts::payer` for
  provider resolution and `money_gates` resolves it again. One indexed
  single-row lookup, taken so exactly ONE place decides what an unowned
  workspace means. Revisit by passing `tenant_id` into `Request`.
- **The KEK reaches `plane.rs`** — one clone at boot into the `Arc<Kek>` every
  sealed-row store shares. `Kek` zeroes on drop.

## Risks / Gotchas

- **§2 IS UNCOMMITTED.** Losing this tree loses the orchestrator.
- **CI has never run this branch.** 37 commits, unpushed, no PR.
- **`make harness-verify` is STAGED-SCOPE.** Nothing staged reads ALL GATES
  GREEN. Read the file COUNT on the UFS row. `git add` first.
- **S6 (350 lines) is NOT gated by harness-verify** and bit four times this
  session. `claim.rs` sits at 349 with no headroom.
- The UFS gate flags numeric literals inside `#[cfg(test)]`; the MS-ID gate
  flags milestone markers in source, doc comments included.
- **The credit metric is a SEAM.** `debit_receive` returns `Deducted(Nanos)`
  rather than metering inline. If §6/M181 never attaches the instrument it
  silently never fires, and no test in either milestone catches it.
- `secrets_map` holds live tenant credentials un-wiped — bound for the wire.
  What IS defended is the realistic leak: `Declared` has a hand-written `Debug`.
- **`KEEP_TEST_STATE=1` breaks `test_migrate_applies_and_reports_success`.**
- **Redis is NOT per-test.** Lease tests derive fleet ids from `process::id()`
  plus a counter.
- **`UZ-RUN-015` must answer 402** — the stock runner classifies a renew
  refusal by BOTH status and code (`control_plane_client_test.zig:42`). This
  matters for §3's renew.

## Decisions taken (do not re-open)

1. **SSRF endpoint refusal is PERMANENT**, diverging from the Zig.
2. **Write 5 (`execution_id`) NOT PORTED**, nor `CLEAR_STALE_EXECUTION`: zero
   production writers of a value, zero readers, one statement that only sets
   NULL behind an `IS NOT NULL` guard. `fleet.runner_leases` already carries
   the fence and TTL that make "executing right now" trustworthy.
3. **Issue-time run-estimate debit NOT IMPLEMENTED (Indy).** The billing
   document contradicts the spec and names M80_010 as having replaced the
   one-shot estimate. The estimate has nowhere to land: `balance_nanos` is
   read-only, `stage` belongs to the renewal accumulator whose
   `… = … + EXCLUDED.…` would DOUBLE CHARGE, and a third charge type breaks the
   two-rows-per-event invariant. **Correct the spec and `data_flow.md` §C.**
4. **Two dead Redis writes NOT ported** — `fleet:gate:pending:` and
   `fleet:gate:notify:`, the latter with no reader anywhere.
5. **No lease wire-version negotiation.** The Zig downgrades a V2 payload for a
   V1 runner because both coexist during a rolling deploy. Rust serves the
   CURRENT shape only, ignores the request body entirely, and offers no
   "unsupported version" refusal — that would need a new registry code, and the
   registry is single-sourced in Zig. Per spec §2 / M175 addendum A1.
6. **No hand-written log wire format.** Telemetry leaves through an OTLP
   collector bound for Elastic and Grafana, so §3 logfmt is the collector's
   business. The daemon installs the stock `tracing_subscriber::fmt` on stderr,
   ANSI only on a terminal, level from `AGENTSFLEET_LOG_LEVEL`.

## Standing instruction — no Zig regressions

**Do not port the Zig's SHAPE where it was a workaround for a missing library.**
Ask: *what could the Zig not reach for, and does this workspace already depend
on something that does it?*

Five confirmed instances, all fixed:

- `egress` — `is_misconfiguration` string-matched `Error::to_string()` against
  three constants → typed `Misconfigured`.
- `policy/build` — the grant set as `&[Box<str>]` with a nested linear scan,
  where `&[]` spelled BOTH "granted nothing" and "never read the grants" →
  `Grants` with no zero-argument constructor.
- `afd_core::id::Uuid7::to_bytes` — a hand-rolled hex nibble decoder written 20
  lines below `Uuid7::encode`, whose own doc says not to → `uuid::parse_str`.
- `afd_crypto::secret::decode_hex_into` — hand-rolled hex on the master-key
  parse path → `hex::decode_to_slice`.
- `policy::repair` — the Zig hand-writes hex AND base64 for the branch name;
  the port uses `uuid` and `base64::URL_SAFE_NO_PAD`.

**Method when replacing:** pin the existing behaviour with tests FIRST, against
the hand-rolled code, and only then swap. The crypto swap proved why — every
pre-existing hex test used the wrong LENGTH, so the digit check had zero
coverage and uppercase acceptance was unasserted.

The audit is TARGETED, not exhaustive; parsing and serialization surfaces have
not been swept. M183_001 is the spec that scopes the full pass.

## Corrections made this session

- **A prior handoff overstated §5** as complete when only the PARSER existed —
  nothing read `core.fleets.config_json`. Do not assume a ✅ means a caller
  exists.
- **The daemon installed no `tracing` subscriber**, so all 97 emits were no-ops
  that did not evaluate their fields. A full boot produced one line, the banner,
  which is `println!`. The logging audit passed throughout because it checks
  call SHAPE, not that anything listens.
- **A workspace test figure was pulled from an earlier handoff before commit**
  because it had been extrapolated rather than measured — the guess was
  818/88, the truth 820/81. Measure it.
