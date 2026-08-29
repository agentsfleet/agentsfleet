<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M183_001: A workaround the Zig needed is not a design the Rust keeps

**Prototype:** v2.0.0
**Milestone:** M183
**Workstream:** 001
**Date:** Aug 26, 2026
**Status:** DONE
**Priority:** P1 — one confirmed finding drops a TLS requirement on the datastore connection; the rest is hygiene that decides how much of the port a reader can trust
**Categories:** API
**Batch:** B7 — parallel with M182; touches no wire field and no schema, so it shares no surface
**Branch:** `fix/m183-sslmode-parsed-not-searched`
**Test Baseline:** `unit=6159 integration=measured at VERIFY` — `make test-unit-all` green at `34387641b` (rustd 1442 + app 2406 + website 175 + cli 1624 + design-system 512). The integration count is taken from the same `make test-integration-rustd` run that grades S3, because this branch's diff is one Rust leaf and the docker lane is a gate there rather than a number here.
**Depends on:** M177_001 (four instances of this defect class were found and three fixed there; this spec sweeps the rest)
**Provenance:** LLM-drafted (Claude Opus 5, Aug 26, 2026)
**Canonical architecture:** `docs/architecture/direction.md` §Two daemons, one contract

---

## Overview

**Goal (testable):** every ported leaf in `rustd/` that re-derives a solved primitive carries a recorded verdict — replaced, unified, kept-with-its-reason-in-the-file, or left — and no swap lands without a test that was proven red against the code it replaced.

**Problem:** `rustd/` is a port of a Zig daemon whose stdlib has no URL parser, no hex codec, no duration type and no set. The Zig hand-rolled each one. Where the port carried that hand-rolled *shape* across as though it were the design, the Rust now honours a constraint that does not exist here. The observable symptom is not "the code is ugly": a Postgres password containing `?sslmode=` currently makes the daemon skip the TLS requirement it means to impose, and two adjacent `i64` cache windows can be handed over in the wrong order with the compiler silent. The second symptom is slower and worse — a reader cannot tell a deliberate hand-roll from an unexamined one, so every future auditor re-derives the same judgement.

**Solution summary:** a bounded audit of the ported leaves, organised by what a wrong answer costs rather than by which grep found it. Three slices carry fixes — the connection-string parser, the integer durations, and the concepts spelled more than once. A fourth writes the reason into every hand-roll that stays, so the judgement stops being re-derived. A fifth records which surfaces were swept and found clean, because on a 56,000-line port the coverage claim is the deliverable and a small finding count is a result, not a failure.

## PR Intent & comprehension handshake

- **PR title (eventual):** `refactor(rustd): a workaround the Zig needed is not a design Rust keeps`
- **Intent (one sentence):** the daemon stops honouring constraints that belonged to a different language, and every place it deliberately still does says so in the file.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_fleet_runtime/src/config/raw/mod.rs` — the three-stage split this whole spec is graded against: serde does shape, garde does bounds, we do meaning. It names the nine hundred Zig lines that interleave all three, and why none of the first two is ours.
2. `rustd/crates/afd_crypto/src/secret.rs` — the split applied to one function. The length check stays hand-written because it carries meaning (`expected` and `actual`, so an operator sees they pasted 63 characters); the digits go to `hex`. Both halves in nine lines.
3. `rustd/crates/afd_core/src/clock.rs` — the port that refuses to port. `clock.zig` exposes a monotonic reading as `i64`; this file leaves it out entirely because `std::time::Instant` makes the mistake unwritable. The model for a `LEAVE` verdict argued rather than assumed.
4. `dispatch/write_rust.md` — RULE ERR-RS and RULE FN-RS, which are the standards any replacement is judged against, plus the four `*.rs` shapes RULE UFS holds out of its count.
5. `rustd/Cargo.toml` — the justification-comment style every dependency entry matches, including two entries (`uuid`, `jiff`) that already record why a crate does one half of a job and not the other.

## Files Changed (blast radius)

**This branch (§1).** Every other row moved below when §2–§5 were parked; see
Discovery §Scope taken this pass.

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_db/src/config.rs` | EDIT | the TLS decision stops being a substring search over the connection string |
| `rustd/crates/afd_db/Cargo.toml` | EDIT | declares whatever §1 resolves the URL with, in the file's existing justification style |
| `rustd/Cargo.lock` | EDIT | one line — `url` becomes a direct dependency of `afd_db`; it was already in the graph through sqlx-core, redis and reqwest |
| `rustd/crates/afd_db/tests/config.rs` | EDIT | the pinning cases landed here first; they moved to the sibling below when the file hit its cap |
| `rustd/crates/afd_db/src/config/tls.rs` | ADD | the scheme table, the sslmode decision, the mode vocabulary and the boot line — carved out when LENGTH GATE fired |
| `rustd/crates/afd_db/tests/config_tls.rs` | ADD | the same cut on the test side; `tests/config.rs` returns to its original 223 lines |
| `docs/v2/done/M183_001_P1_API_ZIG_WORKAROUND_TRANSLITERATIONS.md` | EDIT | the Graded column and the Discovery record are this spec's durable output |

**Follow-up PR (§2–§5), not opened by this branch.**

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_identity/src/capability.rs` | EDIT | the freshness and staleness windows become one typed pair; the swap stops compiling |
| `rustd/crates/afd_identity/src/jwks/cache.rs` | EDIT | the same window type, so the two caches agree by construction |
| `rustd/crates/afd_identity/src/jwks/verifier.rs` | EDIT | the config field that feeds the key cache follows its type |
| `rustd/crates/afd_identity/tests/capability_windows.rs` | EDIT | pinning cases for both windows, and the ordering invariant |
| `rustd/crates/afd_core/src/clock.rs` | EDIT | becomes the one owner of the millisecond-per-second divisor |
| `rustd/crates/afd_fleet/src/gate/store.rs` | EDIT | imports that divisor rather than declaring a second one |
| `rustd/crates/afd_fleet/src/money/nanos.rs` | EDIT | imports it rather than declaring a third under another name |
| `rustd/crates/afd_redis/src/config.rs` | EDIT | `is_tls` reads the scheme table instead of a second inline spelling of `rediss://` |
| `rustd/crates/afd_auth/src/authenticate.rs` | EDIT | the lower-case-hex body predicate becomes the shared one |
| `rustd/crates/afd_core/src/id.rs` | EDIT | owns that predicate, and says why its parse as a whole stays ours |
| `rustd/crates/afd_crypto/src/secret.rs` | EDIT | records why its acceptance of upper-case hex differs from `id.rs` deliberately |
| `rustd/crates/afd_fleet_runtime/src/instructions.rs` | EDIT | the keep marker on the frontmatter delimiter scan |
| `rustd/crates/afd_fleet/src/runner/validate.rs` | EDIT | the keep marker on the allowlist host check |
| `rustd/crates/afd_db/src/migration.rs` | EDIT | the keep marker on the const-evaluated slot parse |
| `rustd/crates/afd_core/src/error_code.rs` | EDIT | the keep marker on the const-evaluated code grammar |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **PSR** (use standard parsers, never hand-roll) is this spec's spine and the rule every §1–§3 finding is filed under. **NTP** (narrow types at parse boundaries) and **TGU** govern what a replacement returns. **UFS** governs §3's restated literals. **TCF** governs the pinning method: a test that survives deletion of its own subject is repaired or deleted. **TFX** binds the pinning tests to production constants. **NLR** (touch-it-fix-it) applies to every file this diff opens. **NRC** bounds §4 — a keep marker earns its place only by preventing a future auditor's re-derivation. **NDC** and **HLP** catch anything a swap orphans.
- `dispatch/write_rust.md` — **RULE ERR-RS** decides how a replacement's failure composes (`#[from]`, never `map_err(|e| Mine(e.to_string()))`, `source()` never returns our own kind); **RULE FN-RS** decides its shape (Result pipeline, parse don't validate, illegal states unrepresentable). Its "Reference guideline" section is mandatory in REVIEW: read the `M-…` sections the diff touches and cite what was applied or diverged from.
- `dispatch/write_any.md` — File & Function Length, UFS, LOGGING and the ERROR REGISTRY delegation, since every file here is a source file.
- `docs/LOGGING_STANDARD.md` — §1 adds the one branch this spec makes observable.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UFS GATE | yes — §3 is entirely restated literals, and §1 may add one | every literal §3 touches ends at one declaration site; `audits/ufs.sh` reads `*.rs` and grades it |
| LENGTH GATE | yes — every touched file is `*.rs` | Rubric S5 reads the count; no file here is near 350 and §4 adds only doc comments, but `make harness-verify` is staged-scope, so `git add` first and read the UFS row's file count rather than trusting a green board on an empty index |
| LOGGING GATE | yes — §1 Dimension 1.4 adds a boot line | one structured event at the resolved-mode branch, per `docs/LOGGING_STANDARD.md`; the connection string never appears in it |
| ERROR REGISTRY | no | `InvalidDatabaseUrl` and `InvalidDatabaseUrlScheme` already exist in `afd_db` and already map to `INTERNAL_DB_UNAVAILABLE`; no code is minted, so the Zig-single-sourced registry is untouched |
| SCHEMA GUARD | no | no file under `schema/` is opened; Invariant 1 makes that mechanical |
| ZIG GATE / UI GATE / DESIGN TOKEN GATE | no | no `*.zig`, no `ui/` |
| MILESTONE ID GATE | yes — this spec adds tests | RULE TST-NAM: no test name or filename carries `M183`, `001`, or a dimension number |
| SPEC TEMPLATE GATE | yes — this file | `bash audits/spec-template.sh --staged` before the authoring commit |
| GREPTILE GATE | yes | the rule IDs above are pre-committed, so the diff obeys them by construction rather than by review |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_fleet_runtime/src/config/raw/mod.rs` — shape/bounds/meaning, the three-stage split. Every verdict in this spec is that question asked of one function: *which stage is this code in, and is it doing a stage that is not its own?* It predicts all four confirmed instances (each was doing shape by hand) and every deliberate keep (each is doing meaning), which is why it replaces "is there a crate for this" as the diagnostic — the crate question has a right answer for `afd_crypto::secret`'s digits and a wrong one for its length check, and the stage question separates them.
- **Reference:** `rustd/crates/afd_fleet/src/money/nanos.rs` — a crate asked for and declined, with the reason named in the file (a decimal would round differently from the Zig and break row-equivalence). The model for §4's output.
- **Reference:** `rustd/crates/afd_auth/src/directory.rs` — the smallest complete instance: a `write!("{:02x}")` loop replaced by `hex::encode`, with the comment saying what the loop risked rather than that it was ugly.

## Sections (implementation slices)

Ordered by what a wrong answer costs, not by which grep found it. §1 changes a security posture; §3 changes nothing observable. Sizing them by grep surface would put those two in one commit.

### §1 — The connection string is parsed, not searched

`afd_db::config::url_declares_sslmode` asks whether the database URL declares `sslmode` by splitting on the first `?`, then on `&`, then on `=`. When the answer is no, the caller upgrades the connection to `PgSslMode::Require`. Three inputs make it disagree with the real parser sqlx then runs, and two of them drop the upgrade: a password containing `?sslmode=` (`postgres://u:p?sslmode=disable@h/db` → reads "declared", skips `Require`, sqlx's own default `Prefer` permits cleartext); the same text in a fragment; and a percent-encoded key (`ssl%6Dode`), which reads "undeclared" and forces `Require` over an operator's explicit `disable` — a boot failure with no legible cause. `url` is already a workspace dependency, used by `afd_fleet/src/provider/endpoint/url.rs`. **Implementation default:** ask the parsed value rather than the string — sqlx has already parsed it into `PgConnectOptions`, so if that type can answer what mode it holds, no second parse is needed and no dependency is added; reach for `url` only if it cannot. Either way the substring search goes.

- **Dimension 1.1** — the three divergent inputs are pinned against the CURRENT function, and pass, before anything is swapped → Test `test_sslmode_detection_pinned_against_hand_rolled` — DONE
- **Dimension 1.2** — a password containing `?sslmode=disable` yields a connection that requires TLS → Test `test_password_bearing_query_syntax_still_requires_tls` — DONE
- **Dimension 1.3** — a percent-encoded `sslmode` key is honoured as declared → Test `test_percent_encoded_sslmode_key_is_honoured` — DONE
- **Dimension 1.4** — every boot-time resolve emits the SSL mode it decided, with the knob name and the role, and never the URL → Test `test_resolved_ssl_mode_is_logged_without_the_url` — DONE

### §2 — A duration is a `Duration`, and two windows are one value

**PARKED — follow-up PR.** No live caller passes the pair in the wrong order or a negative window; see Discovery §Scope taken this pass.

`ProviderCapabilities::with_windows(source, clock, ttl_ms: i64, ceiling_ms: i64)` takes two adjacent same-typed integers whose order the compiler cannot check, and the ordering invariant they depend on — an entry may not be stale-servable before it is refreshable — is asserted nowhere. Its body already pays for the mismatch: `u64::try_from(ceiling_ms.max(0)).unwrap_or(u64::MAX)` before `Duration::from_millis`. `KeyCache::new(source, clock, ttl_ms)` and `verifier::Config::ttl_ms` carry the same integer. `Duration` is std and already used at fifteen sites here; this adds nothing. **Implementation default:** one type holding both windows, whose constructor returns `Result` and refuses `ttl > ceiling` — parse, don't validate, per RULE FN-RS. Scope is `afd_identity` only: the `_ms` fields in `afd_wire`, `afd_redis::session` and `afd_fleet::gate` are stored and wire shapes, and Invariant 1 puts them out of scope.

- **Dimension 2.1** — both windows' current behaviour is pinned at the boundaries (age exactly at TTL, exactly at ceiling, one past each) before the type changes → Test `test_capability_window_boundaries_pinned`
- **Dimension 2.2** — a window pair with the ceiling below the TTL is refused at construction → Test `test_ceiling_below_ttl_is_refused`
- **Dimension 2.3** — the caches read the same window type, so a value built for one is accepted by the other → Test `test_both_caches_take_the_same_window_pair`
- **Dimension 2.4** — a negative or absurd millisecond input is refused where it used to be clamped silently → Test `test_negative_window_is_refused_not_clamped`

### §3 — One concept, one spelling

**PARKED — follow-up PR.** Restated literals; nothing observable changes.

Four concepts are each spelled more than once, and one of them disagrees with itself. `MILLIS_PER_SECOND` is declared privately in `afd_core/src/clock.rs` and again in `afd_fleet/src/gate/store.rs`, and a third time as `MS_PER_SEC` in `afd_fleet/src/money/nanos.rs`. `rediss://` is inline in `afd_redis::Config::is_tls` while `REDIS_SCHEMES` sits above it, so adding a scheme to the table leaves `is_tls` behind. The base64url engine is named at three production modules. And "a lower-case hex body" is written three times: `afd_core::id::first_violation` and `afd_auth::authenticate::accepts_shape` agree (upper-case rejected), while `afd_crypto::secret::decode_hex_into` accepts upper-case because `hex` is case-insensitive — a real difference readable only by opening all three. **Implementation default:** unify the two that agree, and give the one that differs a comment saying so; a shared predicate that quietly changed `secret.rs`'s acceptance would be this spec's own defect class.

- **Dimension 3.1** — one declaration of the millisecond divisor; the other two import it → Test `test_millis_per_second_has_one_declaration`
- **Dimension 3.2** — `is_tls` is derived from the scheme table, so a new TLS scheme is honoured without a second edit → Test `test_is_tls_follows_the_scheme_table`
- **Dimension 3.3** — `id.rs` and `authenticate.rs` share one lower-case-hex predicate, and an upper-case body is refused by both → Test `test_uppercase_hex_body_is_refused_everywhere_it_was`
- **Dimension 3.4** — `secret.rs` still accepts an upper-case master key, and its file says why that differs → Test `test_uppercase_master_key_still_decodes`

### §4 — The keeps carry their reason in the file

**PARKED — follow-up PR.** Doc comments only; the keeps it would mark are not opened by this branch.

Roughly fifteen hand-rolls reviewed under M177 were judged deliberate, and nothing in the repository records that judgement — so the next auditor re-derives it, and the one after that. This slice writes it down at the site. Each keep gets a doc section headed `# Why this is hand-written` naming what a crate could not express: the const-evaluated ones (`afd_db::migration::version_of`, `afd_core::error_code::is_registry_spelling`) because `str::parse` is not `const fn` and the assertion must fail the build; `afd_fleet::runner::validate::registry_host_valid` because a permissive URL parse in an egress allowlist is a hole in the cage; `afd_fleet_runtime::instructions` because the bytes are compared against bytes the Zig produced; `afd_core::id` because `Uuid::parse_str` normalises the upper case this product rejects. Per RULE NRC the marker earns its place only where a future agent would otherwise re-derive the judgement — a keep whose reason is already in the signature gets no marker and is listed in §5 instead.

- **Dimension 4.1** — every file in §5's KEEP set carries the marker, and it names a capability rather than a preference → Test `test_every_keep_names_what_a_crate_cannot_express`
- **Dimension 4.2** — the marker's phrasing is one literal, so the §5 ledger and the audit grep cannot drift apart → Test `test_keep_marker_is_one_spelling`

### §5 — The coverage ledger

**PARKED — follow-up PR.** A ledger over the six slices this branch no longer sweeps.

The audit's value is the swept-clean list, not the finding count. This slice records, in this spec's own body before it moves to `done/`, one row per surface: the grep or inspection that covered it, what it returned, and the verdict. The surfaces are the seven the sweep was scoped to — parsing and serialization; URL, URI and header handling; date, time and duration arithmetic; string casing, trimming and normalization; collection choices; error composition and classification; retry, backoff and rate-limit arithmetic — plus a row for each surface that is clean because the code does not exist yet (`afd_api` mounts no path parameters and the axum `query` feature is deliberately unenabled), so a later reader does not read absence as coverage.

- **Dimension 5.1** — every one of the seven surfaces has a ledger row with its command and its result → Test `test_coverage_ledger_covers_every_declared_surface`
- **Dimension 5.2** — every finding row carries a verdict from the closed set and a `file:line` → Test `test_every_finding_carries_a_verdict_and_a_site`

## Interfaces

```
The closed verdict set — every finding row is exactly one:

  REPLACE  a crate or std type does this whole job; the hand-roll goes
  UNIFY    the hand-roll is right and exists N times; N becomes 1
  KEEP     right, and staying, because a crate cannot express the meaning
           — the file gains "# Why this is hand-written"
  LEAVE    out of scope here — a wire, stored or schema shape (Invariant 1),
           or a surface with no code yet; named, with where it is raised

Unchanged and NOT to be altered by this spec:
  afd_db::config::pool_options   -> Result<PgConnectOptions>   (same signature)
  afd_identity public constructors keep their names; parameter TYPES change
  every afd_wire struct field, every schema/ column, every SQL statement text
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Password carries query syntax | `postgres://u:p?sslmode=disable@h/db` | the parsed value decides; the connection requires TLS and the boot line names the mode |
| Percent-encoded knob | `?ssl%6Dode=disable` | decoded before comparison; the operator's `disable` is honoured rather than silently upgraded |
| Malformed connection string | a URL neither sqlx nor the parser accepts | existing `InvalidDatabaseUrl` with its `source` chain intact — no `to_string()` on the way in (RULE ERR-RS) |
| Inverted cache windows | a caller passes ceiling below TTL | refused at construction with the two values named; the daemon does not boot holding a cache that cannot serve |
| Negative window | a knob or test passes a negative millisecond count | refused, not clamped to zero; the old `.max(0)` silently made it a zero-TTL cache |
| Pinning test cannot fail | a test asserts a shape both implementations satisfy | RULE TCF: deleting the subject clause must turn it red; Discovery records the proof or the test is deleted |
| A swap alters the wire | a replacement rounds, normalises or reorders differently | Invariant 1's grep fails the branch before review sees it |

## Invariants

1. **No wire, stored or schema shape moves.** Enforced by Rubric R5: the branch's diff contains no path under `schema/`, `rustd/crates/afd_wire/`, or `public/openapi/`. A finding that would require one is verdict `LEAVE` with its follow-up named.
2. **A window pair cannot be inverted.** Enforced by the constructor returning `Result` and by there being no other way to build one — not by a call-site check.
3. **A duration and an instant cannot be confused.** Enforced by the type: `Duration` has no epoch, `UnixMillis` is a newtype, and neither converts to the other implicitly. This is `clock.rs`'s existing argument extended to the windows.
4. **Every swap is preceded by a test that ran green on the old code.** Enforced by the commit order — the pinning commit precedes the swap commit for each of §1, §2, §3 — and readable in `git log`, which Rubric R6 grades.
5. **No new error code.** Enforced by the ERROR REGISTRY gate; every failure here reuses an existing `afd_db` or `afd_identity` variant.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `db_ssl_mode_resolved` | ops | a role's connect options are built — every boot-time resolve, not once per role | knob name, role, resolved mode, whether the URL declared it | never the connection string, never the password, never the host's credentials | `test_resolved_ssl_mode_is_logged_without_the_url` |

**Correction to the row above, found in REVIEW.** "Once per role" was an
assumption, not a reading. `PoolConfig::resolve` has three boot-time callers —
`agentsfleetd::preflight` (`preflight.rs:196`, API role), `agentsfleetd::migrate`
(`migrate.rs:33`, migrator role) and `Db::connect_role` (`pool.rs:220`) — so the
API role emits this line TWICE at boot: once when preflight proves the knob
resolves, once when the pool is actually built. Both lines are true and both say
the same thing. It is recorded rather than deduplicated because the fix would be
a flag threaded through a constructor to suppress a log, which is worse than two
honest lines. No caller is on the request path, so the line does not scale with
traffic.

No product signal changes and no funnel moves: every other slice is internal and observable only through the tests above. Metrics review therefore records "no analytics/funnel playbook update required — one operator boot line added, no user-facing event".

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_sslmode_detection_pinned_against_hand_rolled` | the three divergent URLs plus two ordinary ones produce the CURRENT verdicts; committed before any swap |
| 1.2 | unit | `test_password_bearing_query_syntax_still_requires_tls` | `postgres://u:p?sslmode=disable@h/db` → options require TLS |
| 1.3 | unit | `test_percent_encoded_sslmode_key_is_honoured` | `?ssl%6Dode=disable` → the declared mode wins, no forced upgrade |
| 1.4 | unit | `test_resolved_ssl_mode_is_logged_without_the_url` | the captured event carries the mode and no substring of the URL's userinfo |
| 1.x | integration | `test_pool_connects_under_each_resolved_mode` | against the compose Postgres: a URL with and without a declared mode both connect, and the mode is what the boot line said |
| 2.1 | unit | `test_capability_window_boundaries_pinned` | age = TTL → fresh; TTL+1 → stale; age = ceiling → servable; ceiling+1 → refused. Committed before the type changes |
| 2.2 | unit | `test_ceiling_below_ttl_is_refused` | ceiling 60s with TTL 15min → `Err`, naming both values |
| 2.3 | unit | `test_both_caches_take_the_same_window_pair` | one constructed pair is accepted by the capability cache and the key cache |
| 2.4 | unit | `test_negative_window_is_refused_not_clamped` | `-1` → `Err`, where the old code produced a zero-TTL cache that refetched every call |
| 3.1 | unit | `test_millis_per_second_has_one_declaration` | the divisor's declaration count across `rustd/crates/*/src` is 1 |
| 3.2 | unit | `test_is_tls_follows_the_scheme_table` | a TLS scheme added to the table is reported by `is_tls` with no second edit |
| 3.3 | unit | `test_uppercase_hex_body_is_refused_everywhere_it_was` | an upper-case UUID and an upper-case credential body are both refused, as before |
| 3.4 | unit | `test_uppercase_master_key_still_decodes` | an upper-case 64-character key decodes; a 63-character one reports expected and actual |
| 4.1 | unit | `test_every_keep_names_what_a_crate_cannot_express` | every file in §5's KEEP set contains the marker; a KEEP row with no marker fails |
| 4.2 | unit | `test_keep_marker_is_one_spelling` | the marker literal has one declaration site, and the audit reads it from there |
| 5.1 | unit | `test_coverage_ledger_covers_every_declared_surface` | the ledger has a row for each of the seven surfaces, each with a command and a result |
| 5.2 | unit | `test_every_finding_carries_a_verdict_and_a_site` | every finding row names one of REPLACE/UNIFY/KEEP/LEAVE and a `file:line` |
| — | regression | `test_existing_config_and_cache_suites_unchanged` | the pre-existing `afd_db/tests/config.rs` and `afd_identity/tests/capability_windows.rs` assertions still pass verbatim after every swap |
| — | regression | `test_no_wire_or_schema_path_in_diff` | Invariant 1: the branch's changed-file list contains no `schema/`, `afd_wire/` or `public/openapi/` path |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The connection string is no longer searched by substring (§1) | `grep -n "split_once('?')" rustd/crates/afd_db/src/config.rs` | 0 matches | P0 | ✅ `grep -n "split_once('?')" …/config.rs` → 0 matches |
| R2 | No internal window is a bare integer (§2) | — | **parked with §2** | — | ⏸️ follow-up |
| R3 | One declaration of the millisecond divisor (§3) | — | **parked with §3** | — | ⏸️ follow-up |
| R4 | Every keep names what a crate cannot express (§4) | — | **parked with §4** | — | ⏸️ follow-up |
| R5 | No wire, stored or schema shape moved (Invariant 1) | `git diff --name-only origin/main...HEAD \| grep -E '^(schema/\|rustd/crates/afd_wire/\|public/openapi/)'` | no output | P0 | ✅ no `schema/`, `afd_wire/` or `public/openapi/` path in the diff |
| R6 | Every swap was preceded by a test proven red on the old code (Invariant 4) | `grep -cE '^ +red-proof:' docs/v2/*/M183_001_*.md` | `1` — one swap ships; §2 and §3 are parked. Anchored to the Discovery bullet's indent because the unanchored form counted this row's own command | P0 | ✅ `grep -cE '^ +red-proof:'` → `1`; §1's proof observed at the swap |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | ✅ 7 paths changed, every one in the table above |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | ✅ `make harness-verify` → ALL GATES GREEN |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ `make test-unit-all` → exit 0, `✓ All unit lanes passed` |
| S3 | Integration tier green (§1 touches the pool) | `make test-integration-rustd` | exit 0 | P0 | ✅ `make test-integration-rustd` → exit 0, `✓ Integration suite passed (191 tests)` |
| S4 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ `make lint-all` → exit 0, `✓ All lint checks passed` |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -E '\.rs$' \| xargs wc -l \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ no `*.rs` in the diff over 350 lines (max 343) |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `gitleaks detect` → `no leaks found`, 4807 commits scanned |

**Command source rule:** every S-row Verify command is copied **verbatim from `.oracle/orly.json`** (`conform`, `verify.*`) — the same set `orly gate` runs, so the rubric and the mechanical PR gate grade one boundary. The gate BLOCKs a staged pending/active spec whose rubric omits the declared `conform` or `verify.unit` command; a rubric naming a runner the repository does not declare is wrong by construction.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| none — this spec replaces function bodies and constants, never whole modules | `git diff --diff-filter=D --name-only origin/main...HEAD` → no output |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `url_declares_sslmode` | `grep -rn "url_declares_sslmode" rustd/` | 0 matches |
| `MS_PER_SEC` | `grep -rn -w "MS_PER_SEC" rustd/` | 0 matches |
| the second `MILLIS_PER_SECOND` declaration | `grep -rn "const MILLIS_PER_SECOND" rustd/crates/*/src` | 1 match |

## Out of Scope

- **Anything that moves a byte on the wire, a SQL statement, or a stored row shape.** Invariant 1. The `_ms` fields in `afd_wire`, `afd_redis::session` and `afd_fleet::gate` are stored shapes and stay integers here; converting them is a cutover-adjacent change and belongs with the schema, not with this audit.
- **The session-token parser vs `jsonwebtoken`.** `afd_identity::jwt` splits and verifies by hand while `jsonwebtoken` is a workspace dependency used only for the GitHub App mint. It is a real question — and it is an auth-path rewrite with its own threat model, not an audit row. Raised as a follow-up; this spec records the finding with verdict `LEAVE` and does not touch it.
- **A decimal type for money.** Asked and declined already, in `money/nanos.rs`, for a reason this spec agrees with. Reopening it is a schema change.
- **`afd_api` query parameters and content negotiation.** No code yet — axum's `query` feature is deliberately unenabled and the binary mounts no parameterised route. §5 records it as clean-because-absent so a later reader does not mistake that for coverage.
- **The Zig daemon itself.** Nothing under `src/` is opened.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator whose Postgres password happens to contain `?sslmode=`, who has never thought about it, and whose daemon connects over TLS anyway. They never learn this was ever in question; the boot log tells them the mode if they look.
2. **Preserved user behaviour** — every connection string that works today still works and resolves to the same mode, every cached capability answer is served on the same schedule, and every credential that authenticates today still does. The pinning tests in §1 and §2 exist to make that a proof rather than a hope.
3. **Optimal-way check** — the most direct route to moment #1 is §1 alone. §2–§5 are here because the same reading found them and a second pass over the same files costs more than finishing; the gap to the unconstrained optimum is that a truly complete audit would also cover the surfaces §5 marks clean-because-absent, which cannot be audited until they exist.
4. **Rebuild-vs-iterate** — iterate. A rewrite of the port is the opposite of what this needs: the port is largely correct and its correctness is legible in its comments. The work is separating deliberate from unexamined, which requires reading, not rebuilding, and trades no determinism away.
5. **What we build** — one parser swap, one window type, four unified constants, a marker on each deliberate keep, and a ledger.
6. **What we do NOT build** — no shared "audit framework", no lint plugin to catch the next one, no decimal money, no JWT rewrite. A lint that could express this rule would be a rule; it cannot, which is why the ledger is a document.
7. **Fit with existing features** — compounds with M177's four fixes and with M181's cutover, which needs the port trustworthy before the Zig daemon retires. The one thing it must not destabilize is row-equivalence, which Invariant 1 guards mechanically.
8. **Surface order** — N/A — no user surface. The only externally visible change is one operator boot line.
9. **Dashboard restraint** — N/A — no UI. The boot line is a log, not a control, and no panel reads it.
10. **Confused-user next step** — an operator whose daemon now refuses a connection string it once accepted reads the mode and the knob name in the boot line, and `docs/` already documents `sslmode`. No surface is missing.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five sections ordered by what a wrong answer costs. §1 alone changes a security posture and lands as its own commit so it can be reviewed, reverted, and back-ported without carrying four constant renames with it. §4 and §5 are separated from §1–§3 because they add no behaviour and their review question is entirely different — "is this reason true?" rather than "is this swap correct?"
- **Alternatives considered:** (a) *one commit per grep surface*, as the seven-surface taxonomy suggests — rejected, because it puts a TLS fix and a constant rename in the same reviewable unit and no reviewer can hold both standards at once; (b) *fix §1 only and file the rest* — rejected, because the ~15 undocumented keeps are the reason this audit is being run a second time, and leaving them undocumented guarantees a third; (c) *a clippy lint or an `audits/` script that detects hand-rolls* — rejected, because the distinguishing question is whether the code carries meaning a crate cannot express, which is not mechanically decidable; a script would flag every keep in §4 and be suppressed within a milestone.
- **Patch-vs-refactor verdict:** this is a **patch**, deliberately. Every finding is a leaf function whose signature is unchanged or narrowed; nothing here restructures a module or moves a boundary. The one place a refactor is arguably right — `afd_identity::jwt` against `jsonwebtoken` — is named in Out of Scope with its own follow-up rather than mud-patched into this diff.

## Discovery (consult log)

- **Consults** — one gate-flag triage, mechanical, auto-applied per the triage
  rule: LENGTH GATE fired at `config.rs` 352/350 and `tests/config.rs` 529/350.
  Split by concern into `config/tls.rs` and `tests/config_tls.rs`; no judgment
  call, no rule weakened, nothing suppressed. No architecture consult — this
  names no stream, channel, namespace, queue or schema.

- **Reference guideline (mandatory for `*.rs`)** — read sectioned from
  `~/Projects/oss/rust-guidelines/all.txt`:
  - `M-LOG-STRUCTURED` — **applied in part, diverged in part.** Applied: named
    properties, no runtime string formatting, an event identifier. Diverged: the
    guideline wants hierarchical dot-notation (`db.ssl_mode.resolved`) via
    `event!(name: …)`. `docs/LOGGING_STANDARD.md` §8A mandates a snake_case
    `verb_noun` in an `event = …` field, and its EVENT-COMPAT clause keeps a
    port's event spelling stable. Local convention wins; `db_ssl_mode_resolved`
    matches `pool_initialized` beside it rather than being the daemon's only
    dotted event.
  - `M-STRONG-TYPES` — **diverged, named.** `PoolConfig::ssl_mode` returns
    `&'static str`, not `PgSslMode`. The stronger type is foreign (sqlx's), has
    no `PartialEq` or `Display`, and re-exporting it would put a driver type on
    this crate's public surface. The value's purpose is the operator vocabulary
    the log prints, the vocabulary is closed, and `ssl_mode_tag`'s match is
    exhaustive so a variant sqlx adds fails the build.
  - `M-DOCUMENTED-MAGIC` — applied: `SSLMODE_QUERY_KEYS` carries why the pair is
    sqlx's rather than ours.
  - `M-TAUTOLOGICAL-TESTS` — discharged by the mutation run below, not asserted.

- **Diff ledger (`/orly-write-unit-test`, Hardening mode)** — 10 rows, all
  resolved:

  | Changed unit | Test type | Status |
  |---|---|---|
  | `connect_options` signature `knob` → `role` | Behaviour | ✅ `test_each_role_resolves_only_its_own_knob` still proves the error names the knob |
  | `connect_options` declared-true branch | Behaviour | ✅ pinned rows 2–4, `test_percent_encoded_sslmode_key_is_honoured` |
  | `connect_options` declared-false branch (the `require` upgrade) | Behaviour | ✅ pinned rows 1 and 5, `test_password_bearing_query_syntax_still_requires_tls` |
  | `Url::parse` Err arm | Failure | ⛔ `won't-test: unreachable by construction` — `PgConnectOptions::from_str` parses the same string with the same crate first and has already returned `Ok`. Kept as the fail-safe branch, not as a reachable one. |
  | `declares_ssl_mode` true / false | Behaviour | ✅ incl. the negative `?not-sslmode=disable` |
  | `ssl_mode_tag` six arms | Behaviour | ✅ `test_every_declarable_mode_reports_under_its_own_name` — **added by this ledger**, three arms were uncovered |
  | `PoolConfig::ssl_mode` (new `pub`) | Behaviour | ✅ every row above reads through it |
  | `tracing::info!` field set | Behaviour | ✅ `test_the_boot_line_reports_whether_the_url_declared_the_mode` |
  | `tracing::info!` redaction | Failure | ✅ `test_resolved_ssl_mode_is_logged_without_the_url` asserts password, user, host and database are all absent |
  | `InvalidDatabaseUrl` on an unparseable authority | Failure | ✅ the literal-`?`-in-password case |

  Not applicable, with reasons: **concurrency** — `PoolConfig::resolve` is a
  pure read over an injected `EnvSource` with no shared mutable state and no
  boundary crossing; **partial completion** — the change touches no system and
  acquires no resource; **performance** — two parses of one string, three times
  per boot.

- **Mutation (changed lines, targeted; `cargo-mutants` is not installed on this
  machine so the mutants were applied and reverted by hand)** — **5/5 killed, 0
  survivors:**

  | Mutant | Killed by |
  |---|---|
  | drop `"ssl-mode"` from `SSLMODE_QUERY_KEYS` | `test_declared_spellings_are_the_ones_sqlx_honours`, pinning |
  | invert the `declared` branch | 3 tests |
  | restore the substring scan | `test_declared_spellings_…`, `test_percent_encoded_…`, pinning |
  | `ssl_mode_tag` renders `Require` as `"prefer"` | 3 tests |
  | drop the `declared` field from the boot line | both boot-line tests |

- **A correction to this spec, found in EXECUTE.** The Problem statement and the
  Failure Modes table lead with `postgres://u:p?sslmode=disable@h/db` — a
  password carrying query syntax — as the instance that drops TLS. **It is not
  reachable.** That string ends its authority at the `?`, leaving `p` where a
  port belongs, so sqlx refuses the URL before any of this crate's logic runs;
  a `?` in a password must be percent-encoded, and both the old scan and the new
  parse read the encoded form correctly as undeclared. The reachable TLS drop is
  the FRAGMENT case, `…/db#?sslmode=disable`, which resolved to `prefer` and now
  resolves to `require`. `test_password_bearing_query_syntax_still_requires_tls`
  pins all three forms so the claim cannot drift back.

- **Metrics review** — one operator event added, `db_ssl_mode_resolved`. No
  analytics or funnel playbook update required: it is a boot line, no user
  surface reads it, and no product signal moved. The "once per role" wording in
  the Metrics table was an assumption and is corrected above — the API role
  emits it twice at boot because `preflight` and the pool each resolve.
- **Skill-chain outcomes**
  - `/orly-write-unit-test` — Hardening mode. 10-row diff ledger above, all
    resolved. Found one real gap nothing had failed on: `ssl_mode_tag` has six
    arms and the suite reached three, so `allow`, `verify-ca` and `verify-full`
    could have rendered as the wrong word in the boot line. Mutation 5/5 killed.
  - `/orly-write-integration-test` — **not** `N/A`: the diff crosses a module
    boundary with real input/output, because a resolved mode is only a claim
    until a socket agrees with it. `test_pool_connects_under_each_resolved_mode`
    runs against the compose Postgres, which serves no TLS: the lane URL's
    declared `disable` connects, and the same URL with its query stripped
    resolves to `require` and is refused. red-proof: asserting the silent URL
    CONNECTS turns it red with
    `DatastoreUnavailable { source: Tls("server does not support TLS") }`.
    Tiers met: T1 (real pool over real Postgres), T3 (the resolved posture
    asserted either side), T4 (the refusal classified as unreachable rather than
    capacity). T5/T6/T7/T9 not applicable — the surface is a pure resolution
    plus one probe, holds no lease, streams nothing, and is not a daemon.
  - `/review` — one pass. The Codex adversarial pass over the diff returned no
    production-impacting findings and verified the options carrying the resolved
    mode into the event are the same ones handed to the connect probe and the
    lazy pool (`config/tls.rs:76` → `pool.rs:63`). The pass I ran myself found
    one gap and it is now pinned: sqlx compares the query key case-sensitively,
    so `?SSLMODE=disable` is ignored by the driver and this crate correctly
    refuses to count it — behaviour that predates the branch, but was resting on
    nothing.
  - `orly-babysit-prs` — runs after the push.

- **Test Delta** — unit `6159 → 6166` (+7), all in `afd_db`: rustd
  `1442 → 1449`; app, website, cli and design-system unchanged. Integration
  `191 → 192` (+1, the TLS lane). Positive on a code-adding diff, so no
  justification is owed.

- **Headroom note for the follow-up** — `tests/config_tls.rs` sits at 343 of
  350. It is finished for §1, but the next test that lands in it splits the file
  first rather than after.
- **Red-proofs (Invariant 4)** — one expected, not three: §2 and §3 are parked.

  red-proof: §1 — `test_sslmode_detection_pinned_against_hand_rolled` was
  committed green at `3c70aaad0` against the substring scan. Replacing that scan
  with `Url::parse` turned it red on `postgres://u:pw@h:5432/db?ssl-mode=disable`
  (`disable` where `require` was pinned), observed before the rows were updated.
  Re-applying the deleted clause as a mutant turns three tests red, recorded
  above.
- **Coverage ledger (§5)** — parked with §5. This branch swept one surface, and
  its verdict is recorded here rather than in a ledger the other four slices
  would have shared: `afd_db::config::url_declares_sslmode` → **REPLACE**
  (`rustd/crates/afd_db/src/config/tls.rs:110`).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.

### Scope taken this pass — §1 only

The spec was opened with an explicit instruction to take the confirmed defect
and park the rest, and that instruction is the deferral record for §2–§5:

> Indy (2026-08-28 23:20): "Read the @docs/v2/pending/M183_001_P1_API_ZIG_WORKAROUND_TRANSLITERATIONS.md and start the important needed fixes, not all? absolutely needed? other we cn park awhen we move the spec to done" — context: §2, §3, §4 and §5 are parked; §1 is taken in full.

> Indy (2026-08-29 03:05): "dont forget to make the milestone done, with the override from indy saying the other sections are parked" — context: this spec moves to `done/` with `Status: DONE` on §1 alone; §2, §3, §4 and §5 are parked to a follow-up by the same instruction, and the R2/R3/R4 rubric rows read ⏸️ rather than red.

What that buys and what it costs, so the next reader does not re-derive it:

- **§1 is taken** because it is the only slice that changes a security posture.
  Two of its three divergences are reachable through an ordinary connection
  string — `?ssl-mode=disable` and a `#?sslmode=…` fragment — and one of them
  drops the TLS requirement the module documentation promises.
- **§2 is parked.** The swap risk it names is real but latent: every in-tree
  caller of `with_windows` passes the constants in the documented order, and the
  `.max(0)` clamp is reachable only from a caller that passes a negative. No
  production call site does. Filed for the follow-up PR the budget allows.
- **§3, §4 and §5 are parked.** None of them changes an observable behaviour;
  §3 is restated literals, §4 is doc comments, §5 is a ledger over slices that
  are no longer being swept this pass.

The Files Changed table below is scoped to §1 accordingly; the rows for §2–§5
stay listed as the follow-up's blast radius, not this branch's.
