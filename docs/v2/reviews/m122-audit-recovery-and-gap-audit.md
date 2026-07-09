# M122 — Audit recovery, re-verification, and the gap audit

**Date:** Jul 09, 2026
**Provenance:** agent-generated. Recovered from the Jul 02, 2026 `fleet-wide-refactor-audit`
workflow transcripts; re-verified against `main` @ `7a06fb5d`.

This is the evidence record behind `M122_001..005` and `M123_001..003`. It exists because the
original audit's findings were nearly lost, and because a review artifact outlives a scratchpad.

---

## 1. What happened to the Jul 02 audit

`fleet-wide-refactor-audit` (run id `wf_8ec169f4-8e4`) ran **twice** and was **killed both times**.
Combined: ~110 minutes, ~14.2 million tokens, and **`null` returned from both runs**. The script's
only output was its return value, so the abort erased everything it had computed.

The findings survived only because subagent transcripts persist to disk. Reconstructing them from
those transcripts reproduces the workflow's own `91 of 102 findings survived` log line exactly.

| Run | Agents | Findings | Died during |
|---|---|---|---|
| 1 | 95 | 238 raw → 231 deduped | verification, 294 verifiers still queued |
| 2 | 191 | 103 raw → 102 deduped → **91 confirmed** | the coverage-gap round, 3 of 6 finders still running |

### Reproducibility — the load-bearing lesson

Run 2 resumed run 1. Its Find agents were displayed as `cached: true` but **demonstrably re-ran**
(their own full transcripts, different results). Resume is same-session only; run 2 was a different
session.

The same 28 finders, against the same commit, produced **238** findings once and **103** the next
time. Overlap: **25 findings (Jaccard 0.08)** on `(file, line, kind)`; only **9 of 102** run-2
findings fuzzily match any run-1 title.

> A single finder pass samples a small, near-random slice of the real issue space. The union of two
> runs is worth far more than either. `loop-until-dry` was the right shape; a single round is not.

### Design defects in that workflow, for whoever writes the next one

1. **No durable output.** The fatal one. Everything else is survivable.
2. **Degraded-quorum bug.** `verifyOne` kills on `refutes >= 2` keyed to the *intended* vote count
   `n`, not `votes.length`. A P0 whose only surviving verifier refuted it is marked **upheld**.
   It did not bite (all 156 verifiers returned) but it is live.
3. **Dead code.** `const upheld` is computed with one formula and discarded; the returned object
   recomputes it with a different one.
4. **Silent truncation.** `maxItems: 10` bound in **9 of 28** finders in run 1. Nothing logged.
5. **Single-lens verification for P2/P3.** `refutePrompt(f, i % 3)` with `n = 1` always selects
   lens 0 (claim-accuracy). The 69 confirmed P2/P3 findings were **never** asked "is this
   intentional / already mitigated?" or "is the fix worth it?" — the two lenses that later
   overturned findings F09 and F15.

---

## 2. Re-verification of the 28 P0/P1 findings against `HEAD`

`M109_001..004` (all `done/`) were written from this audit and had already remediated much of it.

| Outcome | Count |
|---|---|
| Already fixed by M109 or later | 14 |
| Refuted on re-check | 3 |
| **Still open** | **11** |
| Invalid | 0 |

The three refutations are worth recording, because two of them look real until you read the code:

- **`fleet.runners` has no index.** True, and irrelevant. `fetchPage` wraps the table in an
  unfiltered Common Table Expression referenced twice, which PostgreSQL materializes, and
  `COUNT(*) OVER()` forbids a `LIMIT` short-circuit. A base-table index cannot order a materialized
  result — the proposed index would never be used. P3 hygiene at most.
- **Triplicated `limit` parser.** True, and already decided: `M109_003` §4 chose "correct the
  comment; no convergence" because the three parsers are keyset-cursor based while `parsePageParams`
  is offset based. The stale comment was fixed and regression-locked.
- **`agentsfleet list --limit` doc drift.** The doc matches the authoritative *server* clamp
  (`MAX_LIST_PAGE_LIMIT = 100`); the finding misread the client validator.

The verifiers also deflated most surviving severities. Only two remained P1: the Server-Sent Events
reconnect gap, and the docs describing a continuation cap that exists in zero production files.

The 11 survivors became `M122_001..004`.

---

## 3. The gap audit — three areas never reviewed

The Jul 02 coverage critic flagged six gaps; three finders were still running when it was killed.
Those three areas had never been audited. Re-run Jul 09 (`m122-gap-audit-security`):
**11 findings, 10 upheld, 1 killed.** No P0 or P1 survived severity correction — 8 P2, 2 P3.

| Area | Finding | Votes | Sev |
|---|---|---|---|
| secrets | `crypto_primitives.zig:89` — AEAD called with empty associated data; neither the wrapped Data Encryption Key nor the payload is bound to `(workspace_id, key_name, kek_version)`, and the Key Encryption Key is process-wide | 3/3 | P2 |
| secrets | `crypto_store.zig:30` — plaintext key material never `secureZero`'d on any path | 1/1 | P2 |
| secrets | `crypto_store.zig:73` — zero tests over the envelope lifecycle or its error branches | 1/1 | P2 |
| secrets | `crypto_primitives.zig:82` — random 96-bit nonce under a fixed KEK; NIST SP 800-38D birthday bound (~2^32 wraps) | 1/1 | P3 |
| db | `pool_migrations.zig:275` — bookkeeping `CREATE ... IF NOT EXISTS` DDL runs **before** the advisory lock that exists to serialize migrators | 3/3 | P2 |
| db | `sql_splitter.zig:81` — only bare `$$` recognised; tagged `$body$` and block comments silently mis-split, truncating a migration | 1/1 | P2 |
| db | `pool_migrations.zig:337` — `clearMigrationFailure` runs post-COMMIT outside the transaction and swallows its error; a stale row permanently blocks serve boot | 1/1 | P3 |
| credentials | `broker.zig:113` — cache keyed on `(workspace, integration)` only; a rotated grant keeps serving the old token until expiry | 1/1 | P2 |
| credentials | `integration_oauth_refresh.zig:135` — a rotated `refresh_token` in the provider response is parsed and dropped | 1/1 | P2 |

**The killed finding is instructive.** A reviewer claimed a missing single-flight guard let
concurrent refreshes trip provider reuse-detection. The refuter showed the mechanism was wrong:
the rotated refresh token is never persisted at all, so a rotating provider is already broken
*sequentially* — single-flight would delay revocation by one mint cycle, not prevent it. The
concurrency framing was a red herring; rotation-persistence is the root cause. That correction
became `M123_003`.

These nine findings became `M123_001..003`. The tenth (a stale migration-count assertion) is folded
into `M122_005` instead, for the reason below.

---

## 4. The discovery that outranks the audit: `test` blocks that never run

Chasing the stale-assertion finding turned up something worse.

`src/agentsfleetd/cmd/common.zig` has **11 `test` blocks. None of them execute.**

**Proof, empirical rather than modelled.** Line 112 asserts `migrations.len == 26` and line 228
asserts the last migration version is 26. `canonicalMigrations()` returns
`[schema_migrations.len]`, and `schema/embed.zig` has **27** entries. The assertion is *false*.
`make test-unit-agentsfleetd` reports `1504 pass, 493 skip, 0 fail`. A compiled test with a false
assertion cannot pass. Therefore it is not compiled.

**Cause.** Zig collects a file's inline tests only when the file is reachable from a test root.
`tests.zig` force-imports exactly one file from `cmd/` — `cmd/serve_test.zig` — and neither it nor
`serve.zig` imports `common.zig`. Its only importers are `migrate.zig`, `doctor.zig`, and
`preflight.zig`, none of which any test root reaches. Nothing enforces the convention.

**Two of the 11 dead blocks are the guards for findings in §3 above:**

- `test "every migration SQL is parseable by SqlStatementSplitter"` — guards the `sql_splitter` bug.
- `test "integration: startup blocks on concurrent migration race when lock unavailable"` — guards
  the `pool_migrations` advisory-lock bug.

The guards existed. They never ran. Five of the 11 are labelled `integration:`.

**And the depth gate credits them.** `_lint_zig_test_depth` counts `^test "` textually across
`src/`, so `Test Baseline` / `Test Delta` — the mechanism VERIFY uses to prove a change added
tests — counts blocks that cannot execute.

**Scope beyond `cmd/common.zig` is unresolved, deliberately.** A static reachability walk flags a
large set, but the same model predicts 1735 live blocks where the binary registers 1997 — it
under-predicts liveness, so it over-predicts death. It is an upper bound with false positives, not
a verdict. `M122_005` derives the true set from the compiler instead, and pins no number.

---

## 5. Standing recommendations

- **Persist inside the workflow, not at the end.** Findings that exist only in a return value are
  one abort away from gone. Have finders `Write` their results before returning.
- **Repair guards, never silence them.** `M122_004` (a doc-freshness gate frozen at milestone M51
  and never wired into a make target) and `M122_005` (tests that cannot run) are the same class.
  So is `M122_001` §3: the OpenAPI checks validate only paths already in `openapi.json`, which is
  precisely how four live `/v1/admin/models` routes drifted undocumented.
- **Prefer a gate that enumerates over a sweep that guesses.** A route-coverage invariant retires a
  whole class of parity findings; verifying them one by one retires one each.
