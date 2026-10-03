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

# M210_005: Every bound on untrusted input is declared on its type with garde, a parser accepts only input garde has proved, and every schema a model reads is derived by schemars

**Prototype:** v2.0.0
**Milestone:** M210
**Workstream:** 005
**Date:** Oct 03, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — 56 hand-written input checks answer callers today, ten of them the same `?limit` rule spelled ten ways, and three inputs have no bound at all
**Categories:** API
**Batch:** B2 — folded into M210_002 and shipped in its Pull Request, by Indy's call ("The schemars/garge must be fixed in this mielstone/PR"); the milestone's fifth, cross-cutting workstream
**Branch:** `feat/m210-agent-loop-hosted-tools`
**Folded-into:** `M210_002`
**Baseline revision:** `4339afb59fe83a20fb643004e432b9755e1b14a7`
**Test Baseline:** pending — measured before the Pull Request, with M210_002's
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M210_002 (`afr_tools::Schema::of` and the typed `Handler` adapter, committed `6e7fcea8d`)
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 03, 2026) from Indy's in-session decisions and a source inventory of the branch at `bab2368e0`, re-checked at `b6ede4400` (no inventoried crate changed between them)
**Canonical architecture:** `docs/REST_API_DESIGN_GUIDELINES.md` §"What to do" (bounds on the request type with garde) and §"What NOT to do" (`?limit` through `afd_core::paging`)

---

## Overview

**Goal (testable):** `test_every_list_route_refuses_its_limit_out_of_range` — every list route reads `?limit` through one garde-bounded type carrying that route's ceiling and answers that route's sentence; and `grep -rn "fn detail_for" rustd/crates` finds nothing.
**Problem:** The REST guide says bounds are declared on the request type with garde, yet 56 input checks are hand-written. Ten routes parse `?limit` by hand (`afd_core/src/paging.rs:172`, `afd_runner/src/view.rs:33`, `afd_api_tenant/src/handler/paging.rs:33`, …). Four handlers each hand-write a report-to-sentence mapper. Two constants named `PROVIDER_MAX_BYTES` disagree (64 at `afd_wire/src/admin_catalogue.rs:146`, 128 at `afd_api_tenant/src/handler/tenant/models.rs:39`). A budget is checked by hand because garde's float range admits NaN. `RegisterRequest.labels` and the events `actor`/`actor_prefix` and approvals `gate_kind` filters have no bound. The tool stub still hand-writes a JSON schema.
**Solution summary:** A leaf crate, `afd_validate`, holds what every crate shares: the custom rules garde lacks (finite, NUL-free, ASCII digits, charsets), `Limit` (a `?limit` bounded by the route's ceiling, passed as garde context), and `Sentences` (a route's table from a reported path to its fixed sentence). Each of the 56 checks becomes a garde rule on the type it guards. A check whose rule concerns a trimmed, decoded or canonical value bounds the struct built from that value. A parser that a bound protects takes `garde::Valid<T>`, so it cannot run on unproved input; that is garde's typestate, enforced by the compiler. `afr_tools::Schema` is built only by `Schema::of::<T: JsonSchema>`.

## PR Intent & comprehension handshake

- **PR title (eventual):** the M210 Pull Request's (owned by `M210_002`); this workstream adds input bounds declared with garde and schemas derived with schemars
- **Intent (one sentence):** A caller who sends too much, too little or the wrong shape gets the same refusal from every route, written once beside the field it bounds, and no parser ever sees input that bound has not proved.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/REST_API_DESIGN_GUIDELINES.md` — §"What to do" and §"What NOT to do": the rule this workstream makes true everywhere.
2. `rustd/crates/afd_fleet_runtime/src/config/raw/mod.rs` and `raw/document.rs` — garde bounds beside the fields they bound, and why garde over a checker of our own.
3. `rustd/crates/afd_library/src/validate.rs` — a garde report mapped back to a crate's own variants; `Sentences` generalises it.
4. `rustd/crates/afd_api_operator/src/handler/admin/platform_keys.rs` — `detail_for`, one of the four mappers `Sentences` replaces.
5. `rustd/crates/afd_core/src/paging.rs` — `Paging::parse`, the `?limit` home the guide names.
6. garde 0.23 in the cargo registry — `Valid<T>` and `Unvalidated<T>` in its validate module, and lines 279-286 of garde_derive's emit module, where custom rules run first with no short-circuit.
7. `docs/RUST_ERROR_STANDARD.md` and `dispatch/write_rust.md` §FN-RS — parse, don't validate; preserved variants.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/Cargo.toml`, `rustd/Cargo.lock`, `rustd/crates/afd_validate/` | CREATE | Shared rules, `Limit`, `Sentences`; depends on garde and serde only |
| `rustd/crates/afd_core/` (`Cargo.toml`, `src/paging.rs`, `src/paging/tests.rs`) | EDIT | `Paging::parse` reads its limit through `Limit` with the caller's ceiling |
| `rustd/crates/afd_wire/src/` (`runner.rs`, `activity.rs`, `tool_trace.rs`, `tool_detail.rs`, `admin_catalogue.rs`, `admin_library.rs`, `secret.rs`, `tenant.rs`, `workspace.rs`, `team.rs`, `auth.rs`, `fleet.rs`) and their tests | EDIT | Request and wire types derive `Validate`; one `PROVIDER_MAX_BYTES` |
| `rustd/crates/afd_runner/src/` (`validate.rs`, `bounds.rs`, `view.rs`, `heartbeat.rs`) | EDIT | Registration, policy, binds and capability bounds move to the wire types |
| `rustd/crates/afd_api_tenant/src/handler/` (`paging.rs`, `tenant/`, `fleet/`, `event/`, `approval/`, `schedule*`, `secret.rs`, `connector/callback.rs`) | EDIT | Query, path and body bounds through `Limit`, path types and `Sentences` |
| `rustd/crates/afd_api_operator/src/handler/` (`admin/platform_keys.rs`, `admin/models.rs`, `admin/libraries_request.rs`, `operator/query.rs`) | EDIT | Same |
| `rustd/crates/{afd_tenant,afd_vault,afd_cron,afd_billing,afd_events,afd_connector}/` (`Cargo.toml` and the inventoried files) | EDIT | garde joins the six crates that hand-write every bound today |
| `rustd/crates/afd_library/src/` (`prepare.rs`, `github.rs`, `frontmatter.rs`, `model.rs`), `rustd/crates/afd_fleet_runtime/src/` (`config/trigger.rs`, `name.rs`, `config/policy.rs`, `config/raw/policy.rs`), `rustd/crates/afd_fleet_lifecycle/src/install/authored.rs` | EDIT | Document bounds as garde; parsers take `Valid<T>`; the finite budget |
| `rustd/crates/afd_fleet/src/lease/` (`tool_trace.rs`, `tool_detail.rs`) | EDIT | A report maps back to the drop reason it logs today |
| `rustd/crates/afd_api/tests/` | EDIT / CREATE | Route suites for the limits, paths and filters |
| `rustd/crates/afr_tools/src/` (`schema.rs`, `stub.rs`, `catalog.rs`), `rustd/crates/afr_agent/src/fixture.rs`, `rustd/crates/afr_providers/src/request.rs` | EDIT | `Schema` built only by `Schema::of`; read through accessors |
| `CLAUDE.md`, `docs/REST_API_DESIGN_GUIDELINES.md` | EDIT | The rule beside the error-standard bullet; `Limit` and `Sentences` named in the guide |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — PSR (garde, not hand-rolled checks), UFS (every bound a named constant, one per bound), NDC and ORP (the replaced checks and the four mappers leave no caller), NLR (the duplicate `PROVIDER_MAX_BYTES` and the mis-sentenced bounds below are fixed while touched), EMS (sentences are constants beside their table), TCF, TST-NAM.
- `dispatch/write_rust.md` — §FN-RS (parse, don't validate: the bounded type is built once at the boundary, interior re-checks are deleted), §ERR-RS (each crate keeps its variants; `afd_validate` has no fallible signature, so no error type), §UFS.
- Microsoft Pragmatic Rust Guidelines — `M-SMALLER-CRATES` (`afd_validate` is a leaf), `M-STRONG-TYPES-GUARD` (`Valid<T>`), `M-DI-HIERARCHY` (concrete types and generics; no new `dyn`).
- Indy's standards list (Oct 03): traits and structs with behaviour, few clones, no `Mutex`, `error_shell!`, `docs/LOGGING_STANDARD.md`, `afd_observability`, closures over hand-written loops, no duplicates, no hand-rolled code, popular crates, `afd_core` reuse, smaller crates.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| RUST ERR | yes | Existing variants preserved; a garde report converts through each crate's `error_lifts!` |
| UFS | yes | Bounds and sentences are constants; no literal ceiling in a rule |
| File & Function Length (≤350/≤50/≤70) | yes | `afd_validate` splits rules, `Limit` and `Sentences` into three modules |
| LOGGING | yes — drop reasons | `tool_trace` and `tool_detail` drops keep their `reason` values |
| MILESTONE-ID | yes | Test names and comments carry no milestone ids |
| SCHEMA GUARD / UI GATE | no | No schema or UI file |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_fleet_runtime/src/config/raw/` — bounds beside fields, `garde::Report` naming the refused path; mirrored for every request type.
- **Reference:** `rustd/crates/afd_wire/src/runner.rs` `SelftestReport` with `afd_runner/src/bounds.rs:82-89` — a wire type garde proves, read by the daemon; `RegisterRequest`, `AssignedPolicy`, `ExtraBind` and `CapabilityReport` follow it.
- **Reference:** garde's `Valid<T>` typestate (`garde-0.23.0/src/validate.rs:55-109`): the only way to hold a `Valid<T>` is to have validated it.

## Sections (implementation slices)

### §1 — One shared crate: rules, a bounded limit, a sentence table

`afd_validate` exports `finite` (refuses NaN and both infinities), `nul_free`, `ascii_digits` and a charset rule, each a garde `custom` function. It also exports `Limit`, which parses `?limit`, refuses non-digits and then proves `1..=ceiling` with the ceiling passed as garde context; an empty value means the route's default. `Sentences` is a route's `&'static` table from report path to sentence plus a fallback, so a caller never reads garde's own text. `Paging::parse` takes the caller's ceiling. The four `detail_for`/`entry_detail` mappers become `Sentences` tables.

- **Dimension 1.1** — `finite` refuses NaN, +∞ and −∞ and admits every finite value → Test `test_finite_refuses_nan_and_infinity`
- **Dimension 1.2** — `Limit` refuses 0, ceiling + 1 and non-digits with the route's sentences and maps empty to the default → Test `test_limit_takes_each_routes_ceiling`
- **Dimension 1.3** — `Sentences` answers the first entry whose path the report names, else its fallback → Test `test_sentences_pick_the_reported_path`

### §2 — A bound runs before the parser it protects

garde runs custom rules before built-in ones, with no short-circuit, so a field never carries both a bound and a parsing rule. The bound is garde on a struct; the parser takes `&Valid<ThatStruct>`. Applies to the cron expression, timezone and message (`afd_cron/src/validate.rs:83, :124, :145`), the Slack channel id (`afd_fleet_runtime/src/config/trigger.rs:121`), fleet and credential names (`afd_fleet_runtime/src/name.rs:171-173`), GitHub owner/repo/ref segments (`afd_library/src/github.rs:207`), the SKILL.md name (`afd_library/src/frontmatter.rs:17`), declared requirements (`afd_library/src/prepare.rs:75-82`) and the trigger count (`afd_fleet_runtime/src/config/trigger.rs:250`). The schedule message over 8192 bytes stops answering "must not be empty" and names its cap. The schedule write doc's REQ-002 becomes the REQ-001 the code answers (`afd_api_tenant/src/handler/schedule/write.rs:36`).

- **Dimension 2.1** — A 129-byte cron expression is refused by validation, so the parser is never called, and the route answers today's sentence → Test `test_oversized_cron_is_refused_by_its_bound`
- **Dimension 2.2** — A 65-byte timezone is refused before the tz-database lookup → Test `test_oversized_timezone_never_reaches_the_lookup`
- **Dimension 2.3** — A schedule message over the cap answers a sentence naming the cap → Test `test_schedule_message_over_cap_names_the_cap`

### §3 — Runner and wire inputs

`RegisterRequest`, `AssignedPolicy`, `ExtraBind` and `CapabilityReport` derive `Validate` in `afd_wire`, replacing `afd_runner/src/validate.rs:81, :100, :121, :145, :165, :167, :177` and `afd_runner/src/bounds.rs:125`. `labels` gains a count and length bound. The binds sentence names the count, note and path bounds instead of one sentence for all three. The activity frame `call_id` (`afd_wire/src/activity.rs:174`), trace calls (`tool_trace.rs:158, :162, :178, :253, :265, :273, :283`) and tool-call records (`tool_detail.rs:77, :84-85`) derive `Validate`; each report maps back to the `TraceRejection`/`DetailRejection` and `reason` logged today. The raw 64 KiB trace cap stays before the parse.

- **Dimension 3.1** — Each registration bound refuses at its edge with its sentence; `labels` over its bound is refused → Test `test_register_request_bounds_refuse_with_their_sentences`
- **Dimension 3.2** — A capability report outside its bounds is ignored, as today → Test `test_capability_report_out_of_bounds_is_ignored`
- **Dimension 3.3** — Trace and record rejections keep their log reasons (`too_many_calls`, `too_large`, `malformed`) → Test `test_trace_and_detail_rejections_keep_their_reasons`
- **Dimension 3.4** — A frame `call_id` of 0 or 65 bytes is malformed → Test `test_activity_frame_call_id_is_bounded`

### §4 — Tenant and operator routes

Every `?limit` reads through `Limit` with its route's ceiling and sentences (`afd_api_tenant/src/handler/paging.rs:33`, `fleet/message.rs:68`, `event/query.rs:235`, `approval/query.rs:154`, `tenant/models/input.rs:36`, `tenant/billing.rs:204`, `tenant/workspace/input.rs:43`, `afd_core/src/paging.rs:172`, `afd_runner/src/view.rs:33`, `afd_api_operator/src/handler/operator/query.rs:84`). Filters and path segments become garde structs: `?provider` after normalising (`models/input.rs:70`), `?fleet` (`operator/query.rs:98`), `?event_type` (`:164`), `?name` (`workspace/input.rs:93`), the memory key after decoding (`fleet/memory_request.rs:258`), `event_id` (`event/mod.rs:231`, `event/tool_call.rs:105`), `{provider}` (`platform_keys.rs:168`). `actor`, `actor_prefix` and `gate_kind` gain bounds. The admin library reasons (`libraries_request.rs:82, :89`) become a custom rule. One `PROVIDER_MAX_BYTES` remains, the catalogue's 64, because no stored provider is longer. An over-long `event_id` stops answering "event_id is required".

- **Dimension 4.1** — Every list route refuses 0 and its ceiling + 1, and accepts its ceiling → Test `test_every_list_route_refuses_its_limit_out_of_range`
- **Dimension 4.2** — Each bounded path segment refuses one byte past its bound with its sentence → Test `test_path_segments_are_bounded_on_their_path_type`
- **Dimension 4.3** — `actor`, `actor_prefix` and `gate_kind` past their bound are refused → Test `test_unbounded_filters_now_refuse_oversize`
- **Dimension 4.4** — A reasons object with 33 entries, or a 501-byte reason, is refused → Test `test_library_reasons_are_bounded`
- **Dimension 4.5** — A 65-byte `?provider` is refused with a sentence naming 64 → Test `test_provider_filter_shares_the_catalogue_bound`

### §5 — Account, secret and document inputs

`afd_tenant`, `afd_vault`, `afd_billing`, `afd_events` and `afd_connector` gain garde. Machine name (`afd_tenant/src/cli_credential/machine.rs:71`) and workspace name (`workspace/name.rs:91`) are bounded after trimming; a blank workspace name still generates one. Session fields (`session/input.rs:80, :176`), API key name and description (`apikey/name.rs:41, :75`), invite email (`team/email.rs:52`), secret name on body and path alike (`afd_vault/src/secret.rs:71`), canonical secret data (`:126`), `installation_id` (`afd_connector/src/github.rs:104`) and the id half of a decoded cursor (`afd_billing/src/tenant/cursor.rs:68`, `afd_events/src/history/cursor.rs:78`) become garde rules with today's codes. Authored tags (`afd_fleet_lifecycle/src/install/authored.rs:147, :150`) keep REQ-001. The budget (`afd_fleet_runtime/src/config/policy.rs:54`) is `finite` and ranged.

- **Dimension 5.1** — A 64-character machine name padded with spaces is accepted; 65 characters are refused → Test `test_machine_name_is_bounded_after_trimming`
- **Dimension 5.2** — A blank workspace name still generates one; 129 code points are refused → Test `test_blank_workspace_name_still_generates_one`
- **Dimension 5.3** — Each session field over its bound answers its AUTH code → Test `test_session_fields_keep_their_auth_codes`
- **Dimension 5.4** — A 65-byte secret name is refused on create and on the replace path alike → Test `test_secret_name_is_bounded_on_body_and_path`
- **Dimension 5.5** — A budget of `.nan` or `.inf` is refused as a bound break → Test `test_budget_refuses_nan_and_infinity`
- **Dimension 5.6** — A cursor whose id half is 129 bytes is the one undifferentiated cursor refusal → Test `test_cursor_id_bound_stays_one_refusal`

### §6 — Every model-read schema is derived

`Schema`'s fields become private and `Schema::of::<T: JsonSchema>` its only constructor; providers and the catalog read through accessors. The stub and the loop fixture offer the schema of an empty argument type deriving `JsonSchema` with `deny_unknown_fields`.

- **Dimension 6.1** — The stub's parameters are the schema derived for its empty argument type → Test `test_stub_schema_is_derived` — DONE (`rustd/crates/afr_tools/src/stub/tests.rs`)

## Interfaces

```
afd_validate::finite(&f64, &()) · nul_free(&str, &()) · ascii_digits(&str, &()) · charset(…)   → garde::Result
afd_validate::Limit::parse(raw: Option<&str>, ceiling: Ceiling) → Result<u32, LimitBreak { NotDigits | OutOfRange }>
afd_validate::Sentences { entries: &'static [(&'static str, &'static str)], fallback: &'static str }
                     .pick(&garde::Report) → &'static str
afd_core::paging::Paging::parse(parameter, ceiling)                   (ceiling from the route)
afr_tools::Schema::of::<T: JsonSchema>(description) · .description() · .parameters()   (no struct literal)
cron/timezone/channel/name/segment parsers: fn parse(input: &garde::Valid<T>) -> Result<…>
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Oversized input reaches a parser | A bound and a parser share one field | Parser signatures take `Valid<T>`; the bound's refusal answers (Dimensions 2.1, 2.2) |
| NaN or infinity accepted as a budget | garde's range compares with `<`/`>` | `finite` runs on the field; AGT-008 (Dimension 5.5) |
| garde's own text reaches a caller | A report path the table lacks | `Sentences` answers the route's fallback (Dimension 1.3) |
| A transformed value bounded raw | garde on the unparsed field | The struct is built from the trimmed/decoded/canonical value (Dimensions 5.1, 5.2) |
| A non-numeric limit answers serde's text | Typed query deserialised first | `Limit` parses the digits itself and maps both breaks to sentences (Dimension 1.2) |
| A dropped trace loses its log reason | Report replaces the rejection | The report maps back to the rejection (Dimension 3.3) |
| A pinned sentence changes unnoticed | Conversion rewrites a refusal | Existing pinned suites stay green; only the corrections in §2–§4 change text (rubric S2, S4) |

## Invariants

1. A parser that a bound protects runs only on input garde proved — enforced by the compiler: those parsers take `&garde::Valid<T>`, which only `Unvalidated::validate` constructs.
2. A model reads only schemars-derived schemas — enforced by the compiler: `Schema` has private fields and one constructor, `Schema::of::<T: JsonSchema>`.
3. A refusal's sentence is one of its route's constants, never garde's text — enforced by `Sentences::pick`, which returns only table entries or the fallback; Dimension 1.3.
4. No float input accepts NaN or an infinity — enforced by `finite` on the one float input, the budget; Dimension 5.5.
5. One bound, one constant — enforced by the rubric's single-definition grep on `PROVIDER_MAX_BYTES`, and by `Limit` taking each ceiling from its route's constant.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| No new product or operator signal | — | — | — | — | — |
| `tool_trace` / `tool_detail` drop log `reason` (existing) | ops | A trace or record breaks a bound | `reason` values unchanged | No content | `test_trace_and_detail_rejections_keep_their_reasons` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_finite_refuses_nan_and_infinity` | NaN, +∞, −∞ → error; 0.0, −1.5, `f64::MAX` → ok |
| 1.2 | unit | `test_limit_takes_each_routes_ceiling` | ceiling 25: "0", "26", "abc" → `OutOfRange`/`NotDigits`; "" → default; "25" → 25 |
| 1.3 | unit | `test_sentences_pick_the_reported_path` | report at `host_id` → its sentence; at an unlisted path → fallback |
| 2.1 | unit | `test_oversized_cron_is_refused_by_its_bound` | 129-byte expression → a garde report at `expression`, no `Valid` value to parse; route sentence unchanged |
| 2.2 | unit | `test_oversized_timezone_never_reaches_the_lookup` | 65-byte zone → the bound's refusal; a valid zone still resolves |
| 2.3 | integration | `test_schedule_message_over_cap_names_the_cap` | 8193-byte message → 400 REQ-001, sentence names 8192 |
| 3.1 | unit | `test_register_request_bounds_refuse_with_their_sentences` | `host_id` 257 B, 33 allowlist entries, 6-digit port, 17 binds, 201 B note, 1 B path, labels past bound → each its sentence |
| 3.2 | unit | `test_capability_report_out_of_bounds_is_ignored` | 17 controllers → report ignored, heartbeat accepted |
| 3.3 | unit | `test_trace_and_detail_rejections_keep_their_reasons` | 201 calls → `too_many_calls`; 64 KiB + 1 output → `too_large`; `call_number` 0 → `malformed` |
| 3.4 | unit | `test_activity_frame_call_id_is_bounded` | `call_id` "" and 65 B → malformed; 64 B → ok |
| 4.1 | integration | `test_every_list_route_refuses_its_limit_out_of_range` | each list route: 0 and ceiling + 1 → 400 with its sentence; ceiling → 200 |
| 4.2 | integration | `test_path_segments_are_bounded_on_their_path_type` | memory key 256 B decoded, `event_id` 257, `{provider}` 33 → 400 with each sentence |
| 4.3 | integration | `test_unbounded_filters_now_refuse_oversize` | `actor`, `actor_prefix`, `gate_kind` one past bound → 400 |
| 4.4 | unit | `test_library_reasons_are_bounded` | 33 entries → refused; 501-byte reason → refused; 32 × 500 → ok |
| 4.5 | integration | `test_provider_filter_shares_the_catalogue_bound` | `?provider` of 65 bytes → 400 naming 64 |
| 5.1 | unit | `test_machine_name_is_bounded_after_trimming` | 64 chars + 3 spaces → ok; 65 chars → `CliCredentialMachineNameInvalid` |
| 5.2 | unit | `test_blank_workspace_name_still_generates_one` | "  " → generated name; 129 code points → refused |
| 5.3 | integration | `test_session_fields_keep_their_auth_codes` | each of public_key, token_name, ciphertext, nonce, code past bound → AUTH-016…020 |
| 5.4 | integration | `test_secret_name_is_bounded_on_body_and_path` | 65-byte name on create and replace → REQ-001 both |
| 5.5 | unit | `test_budget_refuses_nan_and_infinity` | TRIGGER.md `budget: .nan` and `.inf` → AGT-008 |
| 5.6 | unit | `test_cursor_id_bound_stays_one_refusal` | 129-byte id half → "invalid cursor", same as a malformed cursor |
| 6.1 | unit | `test_stub_schema_is_derived` | stub parameters == `Schema::of::<NoArguments>` parameters |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | No hand-written report mapper remains (§1) | `grep -rn "fn detail_for\|fn entry_detail" rustd/crates --include='*.rs'` | no output | P0 | |
| R2 | One provider bound (§4) | `grep -rn "const PROVIDER_MAX_BYTES" rustd/crates --include='*.rs' \| wc -l` | `1` | P0 | |
| R3 | Routes refuse with their sentences (§2, §4, §5) | `make test-integration-rustd` | exit 0 | P0 | |
| R4 | Model schemas are derived (§6) | `cargo test --manifest-path rustd/Cargo.toml -p afr_tools test_stub_schema_is_derived` | exit 0 | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from this table or a folded spec's | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Every required check passes before the Pull Request is ready; a P1 ❌ needs an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

| File to delete | Verify |
|----------------|--------|
| `rustd/crates/afd_runner/src/view.rs` `PageLimit`, `afd_api_tenant/src/handler/paging.rs` limit parser | `grep -rn "struct PageLimit\|fn parse_limit" rustd/crates --include='*.rs'` → no output |

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `detail_for`, `entry_detail` | `grep -rn "detail_for\|entry_detail" rustd/crates --include='*.rs'` | 0 matches |

## Out of Scope

- Byte caps checked before anything is typed: the webhook body (`afd_api_ingress/src/handler/webhook/mod.rs:80`), raw trace (`afd_wire/src/tool_trace.rs:236`), tool-call body (`afd_api_runner/src/handler/runner/tool_call.rs:76`), `Upstash-Signature` (`afd_cron/src/verifier.rs:153`), preference body (`afd_api_tenant/src/handler/preference.rs:287`). garde validates a typed value, and these run before one exists.
- Caps on our own output and work (the inventory's budget rows: archive, snapshot, push, page and retry budgets), exact-length format parsers, clamps that never refuse, and config values.
- The archive-size code mismatch (BUNDLE-004 at `afd_library/src/github.rs:216`, REQ-002 at `:178`): a budget, not an input check; raised to Indy.

---
## Product Clarity (authoring record)

1. **Successful user moment** — A caller who sends `?limit=500` to any list gets "limit must be between 1 and 200" or that route's ceiling, worded the same everywhere; an over-long `event_id` is told its length, not that it is missing.
2. **Preserved user behaviour** — Every accepted request is still accepted, and every pinned refusal keeps its code and sentence, except the corrections named in §2–§4.
3. **Optimal-way check** — garde is already the workspace's bound crate and the REST guide's rule; `Valid<T>` makes the ordering a type, not a convention.
4. **Rebuild-vs-iterate** — Iterate: each check moves onto its type; behaviour stays.
5. **What we build** — `afd_validate`, the conversions, the bounds on three unbounded inputs, derived schemas for the stub and fixture.
6. **What we do NOT build** — garde on pre-parse byte caps or on our own budgets; an axum–garde extractor.
7. **Fit with existing features** — Routes, codes and wire shapes are unchanged; `public/openapi.json` is unaffected because no wire field changes.
8. **Surface order** — Shared crate, then parser ordering, then wire, routes, account inputs, schemas.
9. **Dashboard restraint** — N/A — no user surface beyond refusal sentences.
10. **Confused-user next step** — Every refusal names the parameter and its bound, so the fix is in the sentence.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** a fifth workstream folded into the M210 Pull Request: M210_002 is at its 320-line cap and this cuts across 20 crates.
- **Alternatives considered:** shared rules in an `afd_core::validate` module (rejected for `M-SMALLER-CRATES` and Indy's "and smaller crates"; `afd_core::paging` reads `Limit` from the leaf crate instead); `length` plus `custom(parse)` on one field (refused: garde runs the parser on oversized input first); an axum extractor validating every body (not needed: no route derives on a request type it does not already parse).
- **Patch-vs-refactor verdict:** a **refactor** of input checking with three new bounds; no route or wire field changes.

## Discovery (consult log)

- **Consults** — Indy, Oct 03, 2026: "The schemars/garge must be fixed in this mielstone/PR"; chose "Convert everything" for the hand-written input checks, with public sentences free to change where garde cannot match them (superseding "Keep messages identical" for those cases); the standards list, restated Oct 03: "Ensure there are no duplicates, no handrolled code, use of popular standard crates for known pattern of code as opposed to hand rolling, repetitive code is abstracted, and use of afd_core/?" and "and smaller crates".
- **Agent defaults** — "everything" is the inventory's 56 input checks outside garde; pre-parse byte caps and budgets stay hand-written (Out of Scope says why); the provider bound settles at 64; the sentence corrections are the three named in §2–§4.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
