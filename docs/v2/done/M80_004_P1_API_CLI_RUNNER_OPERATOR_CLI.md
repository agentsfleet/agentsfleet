# M80_004: Ship the zombie-runner operator CLI — register / status / doctor + live integration coverage

**Prototype:** v2.0.0
**Milestone:** M80
**Workstream:** 004
**Date:** May 27, 2026
**Status:** DONE
**Priority:** P1 — the runner binary builds and ships (M80_002), but an operator has no way to register a host, mint its `zrn_` token, or inspect it; and no test proves the register → authenticate → call loop end-to-end against a live `zombied`.
**Categories:** API, CLI
**Batch:** B1
**Branch:** feat/m80-004-operator-cli
**Depends on:** M80_002 (the runner binary + distribution pipeline this CLI rides on), M80_001 (the frozen contract the CLI speaks), M80_005 (the `platform_admin`-gated `POST /v1/runners` the `register` subcommand calls)
**Provenance:** agent-generated (Opus 4.7, May 27, 2026), **re-scoped Jun 01, 2026** after an Indy-directed cross-check against `src/runner/`, the CI workflows, and `src/zombied/http/`. The macOS Seatbelt backend is deferred and the distribution pipeline was found already shipped in M80_002 — see Discovery and Out of Scope. The remaining live work is the operator CLI and its integration test.

> **Provenance is load-bearing.** The original draft claimed three slices (macOS sandbox / distribution / CLI). The cross-check found the distribution pipeline already shipped in M80_002 (`63670d09`) and ruled the macOS sandbox out (deprecated facility, dev-only platform). What remains — and is genuinely missing — is the operator CLI and a live integration test. The implementing agent re-verifies every claim against `src/runner/` and `src/zombied/http/`.

**Canonical architecture:** `docs/architecture/runner_fleet.md` (S3 row + the operator-vs-daemon registration model — "Option B").

---

## Implementing agent — read these first

1. `src/zombied/http/route_table.zig` (`specFor`) + `src/zombied/http/router.zig` (`Route` union) — the **register-table dispatch** pattern the runner CLI mirrors: a typed `Command` enum → `CommandSpec { handler, … }`, not ad-hoc branching. This is the "cmd register, same shape as the endpoint register" the operator asked for.
2. `src/zombied/http/handlers/runner/register.zig` + `src/zombied/http/middleware/platform_admin.zig` — `POST /v1/runners` mints a `zrn_<64-hex>` and is gated by `platformAdmin()` (verified `platform_admin` JWT claim; a tenant `zmb_t_` admin key is rejected `403 UZ-AUTH-021`). The `register` subcommand drives exactly this endpoint with exactly this auth.
3. `zombiectl/src/program/help.ts` (`ZombieHelp`) + `zombiectl/test/golden/help-no-color.txt` + `zombiectl/test/golden-output.unit.test.ts` — **the current help system**: ≤80 columns/line, zero ANSI under `NO_COLOR`, no decorative emoji/box-drawing, byte-exact golden fixture. The runner `--help`/`--version`/per-subcommand help conforms to the same output contract and gets its own golden fixture.
4. `docs/CLI_DX_PILLARS.md` — command→handler→renderer split, handler purity, output-as-a-service, structured JSON errors, auto-JSON when piped. The runner CLI obeys these; it mirrors `zombiectl`'s endpoint flag verbatim (UFS), not a bespoke `--mothership`.
5. `src/runner/main.zig` + `src/runner/child_exec.zig` — today the binary recognizes **only** the internal `__execute` re-exec subcommand and otherwise drops straight into the daemon loop; there is **no `--help`, no `--version`, and no operator subcommand layer**. `deploy.sh`'s `is_already_installed()` already calls `zombie-runner --version` — which today falls through to the daemon loop, so the idempotent-skip path silently never fires. Closing `--version` is both a help-system requirement and a deploy.sh-idempotency fix.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** ship zombie-runner operator CLI: register / status / doctor, help-system parity, live register→authenticate integration test
- **Intent (one sentence):** give an operator a first-class way to register a runner host (platform-admin → `zrn_` token + env file), inspect it, and preflight it — and prove the whole register→authenticate→call loop against a live `zombied`.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent and list `ASSUMPTIONS I'M MAKING: …`. Mismatch with Intent → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC (no dead command stubs), UFS (the endpoint-flag spelling shared verbatim with `zombiectl`; command names single-sourced off the `Command` enum so help can't drift).
- **`docs/ZIG_RULES.md`** — the CLI is `*.zig` (tagged-union results, multi-step `errdefer`, cross-compile both targets; `*.zig` ≤350 lines / fn ≤50).
- **`docs/CLI_DX_PILLARS.md`** — handler purity, renderer-owned output, structured JSON errors, auto-JSON when piped.
- **`docs/AUTH.md`** (Runner token → Provisioning) — `register` authenticates with a platform-admin Clerk JWT; tenant admin → `403`.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — the CLI + register table are `*.zig` | cross-compile x86_64 + aarch64; read ZIG_RULES |
| PUB / Struct-Shape | yes — a new `cmd` module with a `pub` dispatch surface | shape verdict per surface; mirror `route_table.zig`'s register shape |
| File & Function Length | yes | one file per subcommand handler; the register table + renderer split if they near the cap |
| UFS | yes — endpoint-flag spelling + command names | share the endpoint flag verbatim with `zombiectl`; derive command names + help rows from the `Command` enum (single source) |
| LOGGING | yes — CLI emits | logfmt with `error_code` on failure; no token in logs (a `zrn_` is a secret) |
| ERROR REGISTRY | yes — CLI transport / auth failures | reuse/extend `UZ-RUN-*` (or a CLI surface code) via the registry + `client_errors.zig` mirror |
| LIFECYCLE | no longer — sandbox apply/destroy is deferred (see Out of Scope) | n/a this workstream |

---

## Overview

**Goal (testable):** a platform-admin runs `zombie-runner register --api <url>` (admin Clerk JWT supplied via `ZOMBIE_TOKEN`/`--token`, same precedence as `zombiectl`) against a live `zombied`, which mints a `zrn_` token via `POST /v1/runners`, writes `/etc/default/zombie-runner`, and the minted token then authenticates a real runner call — asserted end-to-end by `test_runner_register_mints_and_authenticates` (integration tier, spawning the compiled binary against the same `zombied` `make test-integration` already brings up). A tenant `zmb_t_` caller running `register` gets `403`.

**Problem:** the runner binary builds, cross-compiles, and ships (M80_002), but it is operable only by hand-writing an env file and HTTP. There is no `register`/`status`/`doctor` surface, no `--help`/`--version` (so `deploy.sh`'s idempotent-skip never fires), and no test proving a freshly-registered runner's `zrn_` actually authenticates.

**Solution summary:** add an operator CLI to the runner binary behind a typed **command register** (a `Command` enum → `CommandSpec` table, mirroring the HTTP `route_table.zig` register the server already uses): `register` (platform-admin → mint `zrn_` + write env), `status`, `doctor`. Make it conform to the current help system (zombiectl's ≤80-col, NO_COLOR-clean, golden-pinned contract) and close `--version`. Then add a live integration test that runs `zombie-runner register` against the `make test-integration` `zombied`, mints a `zrn_`, and uses it for an authenticated runner call.

---

## Prior-Art / Reference Implementations

- **CLI dispatch** → `src/zombied/http/route_table.zig` `specFor` + `router.zig` `Route` union — the typed-enum → spec-table register the operator CLI mirrors.
- **Registration endpoint** → `src/zombied/http/handlers/runner/register.zig` (`performRegister`, mints `zrn_`) + `middleware/platform_admin.zig` (the `platformAdmin()` gate the `register` subcommand authenticates against).
- **Help system** → `zombiectl/src/program/help.ts` + `zombiectl/test/golden/help-no-color.txt` + `golden-output.unit.test.ts` — the output contract + golden-fixture pattern the runner help adopts.
- **Integration harness** → whatever `make test-integration` already stands up (real `zombied` + Postgres + Redis); the new test execs the `zombie-runner` binary against it.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/cmd/registry.zig` | CREATE | `Command` enum + `commandSpec` table + dispatch (mirror of `route_table.zig`); single source for command names + help rows |
| `src/runner/cmd/register.zig` | CREATE | `register` handler — POST /v1/runners (platform-admin JWT), mint `zrn_`, write `/etc/default/zombie-runner` |
| `src/runner/cmd/status.zig` | CREATE | `status` handler — registration + lease state (human + JSON) |
| `src/runner/cmd/doctor.zig` | CREATE | `doctor` handler — preflight (env present, control plane reachable) |
| `src/runner/cmd/help.zig` | CREATE | help/version **renderer** driven by the register table — ≤80 col, NO_COLOR-clean (handler purity: no I/O in handlers) |
| `src/runner/main.zig` | EDIT | dispatch to the command register before the daemon loop; wire `--help`/`--version` |
| `src/runner/cmd/testdata/help.txt` | CREATE | byte-exact help golden (counterpart to `zombiectl/test/golden/help-no-color.txt`), `@embedFile`d by the drift-guard test — compile-time + cwd-independent |
| `src/zombied/errors/error_entries.zig`, `error_registry.zig` | EDIT | a CLI transport/auth failure `UZ-RUN-*` (or CLI-surface) code if a distinct one is warranted |

> The endpoint-flag string is **single-sourced and shared verbatim** with `zombiectl` (UFS); the implementing agent greps `zombiectl/src/` for the exact spelling rather than inventing one.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** two slices — the operator CLI (§1) and a live integration test (§2) — behind seams M80_002 already built (the binary, the contract client, the distribution pipeline). The CLI uses a typed command register so command names + help rows have one source.
- **Alternatives considered:** (a) a separate Node CLI for the runner — rejected, it duplicates `zombiectl`'s contract client and breaks the single-endpoint-flag convention; (b) ad-hoc `if (eql(argv[1], "register"))` branching like the existing `__execute` check — rejected, it drifts from the server's register-table pattern and lets help fall out of sync with the real command set; (c) name the subcommand `enroll` — rejected, the server verb is `register` everywhere (route `register_runner`, handler `performRegister`, log `"registered"`), so the CLI matches it verbatim.
- **Patch-vs-refactor verdict:** **additive** — a new `cmd` module behind the existing binary entrypoint, plus one new integration test. The only edit to live code is `main.zig` dispatch + `--version` (which also fixes `deploy.sh` idempotency).

---

## Sections (implementation slices)

### §1 — Operator runner CLI (register / status / doctor) + help-system parity — **DONE**

Delivers operator subcommands on the runner binary, dispatched through a typed command register that mirrors the HTTP `route_table.zig` shape. Why: operators need to register and inspect a host without hand-crafting HTTP, and the command set must stay in lockstep with its help output.

> **DONE.** `src/runner/cmd/{registry,register,status,doctor,help,output,args,version}.zig`. Tests (descriptive Zig names): 1.1 `"dispatch resolves --help and rejects an unknown command non-zero"` + `"every Command has a non-empty summary"`; 1.2 `"rejectionError maps 403/401/500"` + the live arm; 1.3 `"renderStatus emits the fleet directive in both audiences"`; 1.4 `"envChecks flags missing api + token…"` + `"doctor verdict is non-zero iff any check failed"`; 1.5 `"help matches the checked-in golden byte-for-byte"` + `"help body is ≤80 cols, ANSI-free…"` + `"version line carries the bare build version"`. `/review` hardened it: `<cmd> --help` no longer performs a live action, and the env file is `chmod(0600)`'d on the fd.

- **Dimension 1.1** — a `Command` enum → `commandSpec` table dispatches `register`/`status`/`doctor`; an unknown command prints help and exits non-zero → Test `test_runner_cmd_register_dispatch`
- **Dimension 1.2** — `zombie-runner register --api <url>` authenticates with a platform-admin Clerk JWT taken from `ZOMBIE_TOKEN`/`--token` (verbatim zombiectl precedence), calls `POST /v1/runners`, receives a `zrn_` token, and writes `/etc/default/zombie-runner`; a tenant `zmb_t_` caller gets `403` and a transport failure returns a structured error with a `suggestion` → Test `test_runner_register_structured_error`
- **Dimension 1.3** — `zombie-runner status` reports registration + lease state as human text, and as JSON when stdout is piped → Test `test_runner_status_human_and_json`
- **Dimension 1.4** — `zombie-runner doctor` preflights env-present + control-plane-reachable and reports each check; a failed check is non-zero with a `suggestion` → Test `test_runner_doctor_reports_checks`
- **Dimension 1.5** — `--help`, `<cmd> --help`, and `--version` render through the table-driven renderer: every line ≤80 columns, zero ANSI under `NO_COLOR`, no decorative emoji/box-drawing; the golden fixture matches byte-exact → Test `test_runner_help_golden_and_width`. Closing `--version` makes `deploy.sh`'s `is_already_installed()` version-skip actually fire.

### §2 — Live register → authenticate integration coverage — **DONE**

Delivers an integration test that proves the whole loop end-to-end against the live `zombied` `make test-integration` already stands up. Why: minting and authentication are the runner's trust boundary; a unit mock proves neither the real `platform_admin` gate nor that a minted `zrn_` authenticates.

> **DONE.** Both arms in `src/zombied/http/runner_register_integration_test.zig` spawn the compiled binary against the live harness (real `serve_runner_lookup` wired so the minted token resolves against `fleet.runners`). 2.1 `"operator CLI: register via the binary mints a zrn_ that authenticates"`; 2.2 `"operator CLI: a tenant zmb_t_ caller cannot register (non-zero exit)"`. `make test-integration` builds the runner binary + exports `ZOMBIE_RUNNER_BIN` first. This arm caught the `control_plane_client.register` use-after-free (segfault on the 201 path), now fixed with `.alloc_always`.

- **Dimension 2.1** — against the live `zombied`, a platform-admin `register` mints a `zrn_`, the env file is written, and that `zrn_` then authenticates a real runner call (a heartbeat `POST /v1/runners/me/heartbeats` — the lightest authenticated runner endpoint — returns success), proving the token is live → Test `test_runner_register_mints_and_authenticates`
- **Dimension 2.2** — the same flow with a tenant `zmb_t_` caller is rejected `403` at `register` and no token is minted → Test `test_runner_register_tenant_admin_forbidden`

> **Read-only call note (updated post-`/review`).** The draft had `status` reuse the heartbeat POST. Greptile flagged that this is a *write* (it bumps `last_seen_at`), so an operator's `status` check could mask a dead runner's liveness. At Indy's direction this was fixed properly rather than deferred: a read-only `GET /v1/runners/me` (`runner_self`, pure SELECT, no bump) was pulled forward from M80_006 — contract `PATH_RUNNER_SELF`/`SelfResponse`, the runner's 5th self-scoped verb (docs/AUTH.md). `status` reads it; the live integration arm authenticates through it.

---

## Interfaces

```
zombie-runner register --api <url> [--token <jwt>] → operator-run; platform-admin Clerk JWT (ZOMBIE_TOKEN env, else --token);
                                                     POST /v1/runners (mints zrn_); writes /etc/default/zombie-runner.
                                                     NOT called by the daemon on boot (Option B).
zombie-runner status   [--api <url>] [--json]      → registration + lease state (auto-JSON when piped)
zombie-runner doctor   [--api <url>]               → preflight: env present, control plane reachable
zombie-runner --help | <cmd> --help                → table-driven help; ≤80 col; NO_COLOR-clean; golden-pinned
zombie-runner --version                            → version string (closes the deploy.sh is_already_installed gap)

Admin-JWT resolution (register only): ZOMBIE_TOKEN env (preferred) → --token flag. Same precedence + shell-history
warning as zombiectl; the binary does NOT read zombiectl's credentials.json. The zrn_ for status/doctor and the daemon
comes from ZOMBIE_RUNNER_TOKEN. Endpoint flag --api + env ZOMBIE_API_URL shared verbatim with zombiectl (UFS).

Command register (internal):
  pub const Command = enum { register, status, doctor };           // single source for names + help rows
  fn commandSpec(cmd: Command) CommandSpec { handler, summary, … } // mirror of route_table.specFor
Endpoint flag: the SAME spelling zombiectl uses (UFS — shared verbatim), not --mothership.
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| `register` by a non-platform-admin | tenant `zmb_t_` admin key / missing claim | `403 UZ-AUTH-021` from the server; CLI surfaces a structured error, non-zero exit; no token minted, no env written |
| CLI control-plane unreachable | `zombied` down / bad endpoint | structured JSON error with `suggestion`/`retry`; non-zero exit; no stack dump |
| `register` partial write | env-file write fails after mint | report the minted `runner_id` + the write error so the operator can recover; never silently drop the token (it is returned once) |
| CLI run with stdout piped | LLM/script consumer | auto-JSON (Pillar) — never human text into a pipe |
| `--version` missing (today) | binary ignores args, enters daemon loop | **fixed here** — `--version` returns the version string so `deploy.sh` idempotency works |

---

## Invariants

1. The runner CLI shares the endpoint-flag identifier with `zombiectl` verbatim — enforced by UFS (a single shared spelling; a divergent literal trips the gate).
2. Command names and help rows derive from the `Command` enum / register table — a command with no help row (or vice versa) is impossible by construction; the golden fixture catches drift.
3. `register` mints a token only through the `platform_admin`-gated `POST /v1/runners`; the CLI never mints locally and never logs the `zrn_` — enforced by `test_runner_register_tenant_admin_forbidden` + the LOGGING gate (no secret in logs).
4. Handlers contain no direct I/O (`process.exit`/`stdout`-equivalent) — output flows through the renderer — enforced by the CLI-pillars handler-purity check.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_runner_cmd_register_dispatch` | each `Command` routes to its handler; unknown command → help + non-zero exit |
| 1.2 | unit | `test_runner_register_structured_error` | unreachable endpoint → JSON error with `suggestion`, non-zero exit; tenant key path → 403 surfaced, no env written |
| 1.3 | unit | `test_runner_status_human_and_json` | `status` piped → JSON; tty → human; both report the same state |
| 1.4 | unit | `test_runner_doctor_reports_checks` | env-missing → that check fails non-zero with a `suggestion`; all-present + reachable → success |
| 1.5 | unit | `test_runner_help_golden_and_width` | `--help` under `NO_COLOR` matches the golden byte-exact; every line ≤80 cols; zero ANSI; `--version` prints the version |
| 2.1 | integration | `test_runner_register_mints_and_authenticates` | live `zombied`: platform-admin `register` → `zrn_` minted + env written → that `zrn_` authenticates a heartbeat (success) |
| 2.2 | integration | `test_runner_register_tenant_admin_forbidden` | live `zombied`: tenant `zmb_t_` caller → `register` rejected `403`, no token minted |

Regression: the existing `__execute` child-exec path and the daemon loop are unchanged — adding a command register in front of them must not alter their behavior; M80_002's runner tests stay green.

---

## Acceptance Criteria

- [x] Operator can register a host and the minted token authenticates — live arms 2.1 + 2.2 pass (28/28) against the harness
- [x] CLI dispatch + help/version land — registry-dispatch test + help golden + `zombie-runner --version` → `zombie-runner 0.37.0 (git …)`
- [x] Runner help conforms to the current help system — golden byte-exact; every line ≤80 cols (max 72); zero ANSI under `NO_COLOR`
- [x] Runner CLI auto-JSON when piped — verified: `status --json` → `{"ok":..,"data":..}`; piped (non-TTY) auto-selects JSON
- [x] Both binaries cross-compile both arches — `zig build`/`build_runner.zig` × `{x86_64,aarch64}-linux` all exit 0
- [x] `make lint-zig` clean (per-commit) · runner unit suite green · live arms green · `gitleaks` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: CLI dispatch + help unit — zig build --build-file build_runner.zig test -Dtest-filter="runner_cmd" && echo PASS || echo FAIL
# E2: Build  — zig build && zig build --build-file build_runner.zig
# E3: Help golden + width — NO_COLOR=1 zig-out/bin/zombie-runner --help | awk '{ if (length > 80) print "OVER "NR": "length }'
# E4: Version closes deploy gap — zig-out/bin/zombie-runner --version
# E5: Live loop — make test-integration   (runs test_runner_register_mints_and_authenticates)
# E6: Lint   — make lint 2>&1 | grep -E "✓|FAIL"
# E7: Cross-compile — zig build --build-file build_runner.zig -Dtarget=aarch64-linux 2>&1 | tail -3
# E8: Gitleaks — gitleaks detect 2>&1 | tail -3
# E9: 350-line gate (exempts .md) —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
```

---

## Dead Code Sweep

N/A — no files deleted. The command register is added in front of the existing `__execute`/daemon dispatch (RULE NLR: the new `--version`/`--help` branches replace the implicit fall-through, not leave a dead stub).

---

## Discovery (consult log)

> Append consults, skill-chain outcomes, and Indy-acked deferral quotes as work proceeds.

- **Provenance (May 27, 2026):** authored alongside M80_003/005/006 at Indy's request to formalize the remaining runner-rollout roadmap as specs before M80_002's CHORE(close).
- **Re-scope cross-check (Jun 01, 2026):** verified against `src/runner/`, `.github/workflows/release.yml`+`deploy-dev.yml`, `deploy/baremetal/deploy.sh`, and `src/zombied/http/`. Findings: (1) the distribution pipeline (cross-arch both binaries, idempotent arch-aware `deploy.sh`, systemd unit, env provisioning) **already shipped in M80_002 `63670d09`** — dropped from scope. (2) `establishSandbox` fails closed on macOS by design (`child_supervisor.zig`); the `register_runner` route is already `platformAdmin()`-gated (`route_table.zig:128`) and the verb is `register` (not `enroll`) throughout the server. (3) `zombie-runner` has no `--version` today, so `deploy.sh`'s idempotent-skip silently never fires — folded into §1.5.
- **macOS Seatbelt deferral (Indy-acked, Jun 01, 2026):**
  > Indy (2026-06-01): "I defer macos seatbelt since its deprecated." — context: the original §1 macOS Seatbelt enforcement tier. macOS is treated as a first-class **developer workstation**, Linux as the **authoritative runtime** (the model used by Kubernetes tooling / Terraform / cloud-native dev tools). The `macos_seatbelt` tier stays declared-but-fail-closed; dev on macOS uses `dev_none`. Linux-specific runtime assumptions (cgroups, Landlock, namespaces, systemd) are expected and acceptable — macOS need not reach feature parity.
- **Decisions (Jun 02, 2026, agent, convention-aligned):** (a) endpoint flag is `--api` + env `ZOMBIE_API_URL` — verbatim from `zombiectl/src/program/cli-tree.ts` (UFS), not the draft's `--endpoint`. (b) `register`'s admin JWT resolves `ZOMBIE_TOKEN` env → `--token` flag (zombiectl precedence); the Zig binary does **not** read zombiectl's `credentials.json`. (c) the live arm spawns the compiled binary (Indy's intent) rather than an in-process HTTP call.
- **`/write-unit-test` (Jun 02, 2026):** ledger resolved — pure surface (rejectionError, envChecks, renderStatus, version line, help golden, output envelope, registry dispatch/exhaustiveness, doctor verdict) fully unit-tested; I/O handler paths covered by the live arms; `args.*` + register's missing-input early returns are `won't-test` (read process-global argv/env, not injectable per-test) — exercised end-to-end by the arms. Added `doctor.allOk` exit-code test. Negative-path ratio ≥50%; leaks caught by `std.testing.allocator` (suite green).
- **`/review` (Jun 02, 2026):** independent fresh-context adversarial pass. Memory/lifecycle + the `.alloc_always` fix + `res.body` freeing + `Parsed` lifetime reviewed **clean**. Three findings fixed (commit `8bd6fa24`): **P1** `<cmd> --help` performed a live action (now intercepted in `registry.dispatch`); **P1 (VLT)** 0600 only applied on file creation → `chmod(0600)` the fd; **P2** `envOrDefault` masked OOM as default-tier → switch only on `EnvironmentVariableNotFound`.
- **Use-after-free found+fixed (Jun 02, 2026):** the live arm segfaulted on the 201 path — `control_plane_client.register` parsed with `.alloc_if_needed`, so `runner_id`/`runner_token` sliced into the freed `res.body`. Fixed with `.alloc_always` (commit `f0249fa3`). The binary-spawn integration test is what caught it.
- **Local verification note (Jun 02, 2026):** this env's `rediss://` cert is untrusted by Zig `std.crypto.tls` (the pre-existing `redis_test` `CertificateSignatureInvalid`), which makes **all** harness-backed tests skip locally in non-CI mode. The live arms were therefore run with a plaintext-Redis `REDIS_URL_API` workaround (28/28); CI runs them on the real TLS Redis. The 19 failures in a full local `make test-integration` are pre-existing infra issues (Redis TLS, DB role matrix, svix timestamp) in subsystems this branch never touched (empty diff confirmed).
- **Follow-up to flag:** `release.yml`/`deploy-dev.yml` should pass `-Dgit-commit` to the `build_runner.zig` build so `--version` shows the SHA in CI (cosmetic — the version number is already correct from the `VERSION` build option; the `deploy.sh` idempotency check only needs the number). CI-workflow edit → left for an Indy nod.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | audits coverage vs this Test Specification (esp. the JSON/pipe arms + the live integration loop) | ✅ ledger resolved; added `doctor.allOk` test — see Discovery |
| After tests pass, before CHORE(close) | `/review` | adversarial review vs CLI pillars, ZIG_RULES, the register-table invariants, AUTH.md | ✅ 3 findings fixed (2 P1, 1 P2) in `8bd6fa24`; memory/lifecycle clean — see Discovery |
| After `gh pr create` | `/review-pr` | review-comments the open PR | pending PR open |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Runner unit tests | `zig build --build-file build_runner.zig test` | all green (incl. new cmd + doctor-verdict tests) | ✅ |
| Integration (live loop) | `make test-integration` (filtered live arms) | 28/28 — register→mint→authenticate + tenant 403 | ✅ |
| Runner CLI help golden | `NO_COLOR=1 zombie-runner --help` | byte-exact golden; max line 72 ≤ 80; 0 ANSI | ✅ |
| Runner CLI JSON | `zombie-runner status --json` | `{"ok":false,"error":{…}}` / `{"ok":true,…}` | ✅ |
| Version (deploy gap) | `zombie-runner --version` | `zombie-runner 0.37.0 (git unknown)` — contains VERSION | ✅ |
| Lint | `make lint-zig` (per-commit) | ZLint + pg-drain + FLL + format all pass | ✅ |
| Cross-compile | `zig build … -Dtarget={x86_64,aarch64}-linux` (both binaries) | all 4 exit 0 | ✅ |
| HARNESS VERIFY | `make harness-verify` (per-commit) | UFS/SPEC/ERROR-REG/LOGGING/LIFECYCLE all green | ✅ |
| Secrets | `gitleaks detect` | no leaks; `zrn_`/JWT never logged (RULE VLT) | ✅ |

---

## Out of Scope

- **macOS Seatbelt sandbox backend — DEFERRED** (Indy, Jun 01, 2026). The `sandbox-exec`/Seatbelt facility is deprecated; macOS is a developer workstation, Linux the authoritative runtime. The `macos_seatbelt` tier stays fail-closed (`establishSandbox` → `error.SandboxUnavailable` on non-Linux); dev uses `dev_none`. Revisit only if a sandboxed macOS runtime target ever materializes (likely via a Linux VM reusing the Landlock path, not a hand-rolled Seatbelt backend).
- **Distribution & CI pipeline — DONE in M80_002** (`63670d09`): cross-arch builds of both binaries, idempotent arch-aware `deploy.sh`, `zombie-runner.service`, `/etc/default/zombie-runner` provisioning. Only the `--version` idempotency fix is pulled forward (§1.5); live host bring-up is gated on `DEV_WORKER_READY` provisioning, not this workstream.
- Operator-assigned trust + workspace authz — M80_005.
- Fleet inventory / heartbeat reassignment / lease renewal — M80_006. (The read-only runner `GET /v1/runners/me` was pulled forward into this workstream — see the Read-only call note.)
- Placement scheduler / autoscale — M80_007.
