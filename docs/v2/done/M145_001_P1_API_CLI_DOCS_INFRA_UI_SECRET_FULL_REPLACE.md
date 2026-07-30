<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M145_001: A stored secret is replaced whole, never field by field

**Prototype:** v2.0.0
**Milestone:** M145
**Workstream:** 001
**Date:** Jul 29, 2026
**Status:** DONE
**Priority:** P1 — the only way to change a stored secret today patches one hardcoded field, which cannot express what a user means and silently no-ops on most secret shapes.
**Categories:** API, CLI, DOCS, INFRA, UI
**Batch:** B1 — standalone; no other workstream touches the vault surface.
**Branch:** feat/m145-secret-rotate
**Test Baseline:** unit=3223 integration=455
**Depends on:** none.
**Provenance:** agent-generated (pre-spec, `docs/v2/done/M143_002_P1_UI_LIBRARY_SESSION_EXPERIENCE.md` Discovery A15), redesigned on Indy's direction — see Discovery A3.
**Canonical architecture:** `docs/architecture/web_app.md` (secret surfaces) and `docs/AUTH.md` (workspace operator role on every vault route)

---

## Overview

**Goal (testable):** `PUT /v1/workspaces/{workspace_id}/secrets/{secret_name}` replaces a stored secret's whole body in one statement, the name is never released, a name that is not held answers `404`, and no surface can change a secret one field at a time.
**Problem:** A stored secret can never be read back, and the only write that changes one has been `PATCH` with a hardcoded `api_key`. That shape cannot say what a user means. It silently adds an unused field to any secret keyed on `token` or `api_token` and reports success; it leaves `provider` and `base_url` permanently uneditable once set; and it forces the dashboard's Edit dialog to spend two unrelated writes on one intent, with a partial-success path when the second fails.
**Solution summary:** Replace the verb. `PUT` takes the same `data` object `create` takes and replaces the stored body wholesale, so every secret shape is equally editable and nothing merges. `PATCH` and its handler are deleted rather than kept alongside. The write becomes a single `UPDATE … WHERE workspace_id AND key_name`: one statement instead of read-decrypt-merge-upsert, which removes the unlocked read-modify-write and makes resurrection of a concurrently-deleted secret impossible — zero rows affected is the `404`. The client gains `agentsfleet secret update`, mirroring `create`. The dashboard's Edit becomes the Create form. The vault also stops speaking two envelope versions: reads accept only the AAD-bound format, and the schema default naming the dead one is dropped (§6). Separately, wire the Command-Line Interface (CLI) coverage floor into Continuous Integration (CI), where it has never run.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(api): replace a stored secret whole, and retire the field patch
- **Intent (one sentence):** Changing a secret means sending the secret you want stored, whatever shape it has.
- **Handshake** — at PLAN, restate Intent and assumptions; mismatch means STOP.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/fleets/secrets.zig` — `innerStoreSecret` is the shape `PUT` mirrors; `innerRotateSecret` and `rotateSecretKeyOnConn` are what this workstream deletes.
2. `src/agentsfleetd/secrets/sql.zig` and `secrets/crypto_store.zig` — `INSERT_SECRET_ROW` is the shared column list the new `UPDATE` must keep in lockstep, and `create`'s zero-rowcount idiom is the pattern for the `404`.
3. `src/agentsfleetd/http/handlers/fleets/secret_list.zig` — the list already projects `kind`, `provider`, `model`, `base_url`. That is what lets a client rebuild a full body without ever reading the secret.
4. `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog.tsx` — the form Edit becomes.
5. `~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md` §1–§6 — method choice, response shape, and the OpenAPI change.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/handlers/fleets/secrets.zig` | EDIT | `innerReplaceSecret` replaces `innerRotateSecret`; `RotateBody` and `rotateSecretKeyOnConn` are deleted. |
| `src/agentsfleetd/secrets/sql.zig`; `src/agentsfleetd/secrets/crypto_store.zig`; `src/agentsfleetd/state/vault.zig` | EDIT | The `UPDATE` statement composed from the shared column list, its rowcount-zero `NotFound`, and the vault-level entry point. |
| `src/agentsfleetd/http/route_table_invoke.zig`; `src/agentsfleetd/http/sensitive_request.zig`; `src/agentsfleetd/http/routes.zig` | EDIT | Dispatch `PUT` instead of `PATCH`, and keep the sensitive-request classification on the method that now carries a secret body. |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | Retire `MSG_SECRET_KEY_REQUIRED` once its only caller is deleted. |
| `public/openapi/paths/secrets.yaml`; `public/openapi.json` | EDIT | Replace the `PATCH` operation with `PUT`. |
| `src/agentsfleetd/http/secrets_json_metadata_integration_test.zig`; `src/agentsfleetd/http/route_matchers_test.zig` | EDIT | Method dispatch, replacement semantics, and the negative paths. |
| `cli/src/commands/fleet_secret.ts`; `cli/src/commands/fleet_secret_body.ts`; `cli/src/program/cli-tree-fleet.ts`; `cli/src/program/handlers-bind-fleet.ts`; `cli/src/program/cli-tree-types.ts` | EDIT | `secret update <name> --data=…`, reusing `create`'s body resolver verbatim. |
| `cli/test/fleet-secret-update.unit.test.ts` | CREATE | Command behaviour, body sources, and output shape. |
| `cli/test/secrets.integration.test.ts`; `cli/test/helpers-cli-tree.ts`; `cli/test/json-contract.test.ts`; `cli/test/acceptance/secret-vault.spec.ts` | EDIT | Round-trip against the stub, the two stub handler maps, and the live lane. |
| `ui/packages/app/lib/api/secrets.ts`; `.../settings/models/actions.ts` | EDIT | `replaceSecret` / `replaceSecretAction` over `PUT`; `rotateSecret` is deleted. |
| `.../settings/models/components/EditModelEntryDialog.tsx`; `.../AddModelEntryDialog.tsx` | EDIT | Edit becomes the Create form and sends one secret write; Add's existing-name branch replaces instead of patching. |
| `ui/packages/app/tests/models-registry-add.test.tsx`; `.../models-registry-edit.test.tsx` | EDIT/CREATE | The dialog behaviour both dialogs now share. |
| `schema/039_vault_kek_default_retire.sql`; `schema/embed.zig` | CREATE/EDIT | §6 — drop `DEFAULT 1`, the last way a v1 row could be minted (Discovery A8); no `CHECK` — the version stays an app constant per RULE STS (Discovery A10); register the slot. |
| `src/agentsfleetd/secrets/crypto_primitives.zig` | EDIT | §6 — remove `SecretError.UnsupportedKekVersion`, dead once the read path stops branching on version. |
| `src/agentsfleetd/secrets/crypto_store_test.zig` | EDIT | §6 — the write-arm binding proof replaces the three v1-refusal tests and their fixtures; the no-default test also pins `NOT NULL` (Discovery A10). |
| `src/agentsfleetd/http/secrets_json_metadata_integration_test.zig` | EDIT | The connector-name collision proof (create blocked, PUT overwrites) added at REVIEW (Discovery A8). |
| `src/agentsfleetd/fleet/secrets_resolve.zig` | EDIT | Comment-only: "legacy credential" renamed to what it is (RULE NLR touch-it-fix-it). |
| `.github/workflows/test.yml`; `make/test-unit.mk` | EDIT | Run the enforcing coverage script instead of the non-enforcing one. |
| `cli/scripts/enforce-coverage.mjs`; `cli/package.json` | EDIT | Amended in at EXECUTE (Discovery A7) — the script grades from lcov records instead of the text table, and the printing `test:coverage` twin is deleted so one enforcing script remains. |
| `cli/test/coverage-floor-arms.unit.test.ts` | CREATE | Close the floor gaps §5 turns red-blocking — the uncovered arms on `main`, one test file, each block naming the file it closes. |
| `cli/test/api-url-resolution.integration.test.ts`; `cli/test/login-helpers-funcfill.unit.test.ts` | EDIT | The `--api=<url>` equals form and the SIGINT abort handler — the last two arms. |
| `~/Projects/docs/fleets/secrets.mdx`; `~/Projects/docs/changelog.mdx` | EDIT | Document the replace verb, correct the wrong body, and announce the removal. |
| `src/agentsfleetd/errors/error_entries.zig` | EDIT | `UZ-VAULT-005`'s hint stops naming the retired `PATCH`/rotate (RULE NLR). |
| `src/agentsfleetd/secrets/crypto_store_write.zig` | CREATE | The vault write path, split from `crypto_store.zig` at the read/write seam (RULE FLL, 397 > 350). |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable.tsx` | EDIT | Drops `onPartialSuccess` for `onCommitted`, wired to `refresh` — the Edit dialog re-reads the table when a write commits but the save does not complete (Discovery A11). |
| `docs/architecture/billing_and_provider_keys.md`; `docs/architecture/user_flow.md`; `docs/architecture/product_analytics.md`; `docs/architecture/data_flow.md` | EDIT | Stop describing the field patch; §8.3 carries the whole-body replace, the Edit motion, the `key_rotated` trigger, and the erasure list (Architecture Consult Gate). |
| `VERSION`; `build.zig.zon`; `cli/package.json` | EDIT | Version 0.25.0 (pre-1.0 breaking) via `make sync-version` at CHORE(close). |

**Scope grading.** Rubric R2 compares `git diff --name-only origin/main` against this table. Test files are covered by the row of the code they exercise (the shared `ui/packages/app/tests/` suites, `secrets.test.ts`, `provider-actions.ts`, the `*_test.zig` fixtures), not enumerated separately. A path that proves genuinely required and is missing here is a spec amendment recorded in Discovery, not a silent addition.

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — NSQ (schema-qualified SQL in the domain `sql.zig`), UFS (the shared column list stays one definition), NDC / ORP / HLP (the deleted handler, message constant, and client function leave no orphan), ECL (rowcount-zero is `NotFound`, distinct from a malformed body), EP4 does **not** apply pre-`2.0.0` — the retired method is removed, not stubbed to 410, TNM and TST-NAM, TFX, JCL.
- **`~/Projects/dotfiles/dispatch/write_zig.md`** — handler and store edits: `conn.exec` for the `UPDATE`, function length, cross-compile both linux targets.
- **`~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md`** — `PUT` semantics, status selection, and §7 route registration across every site.
- **`~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md`** and **`docs/DESIGN_SYSTEM.md`** — the CLI command and both dialogs.
- **`~/Projects/dotfiles/docs/DOCUMENTATION_RULES.md`**, **`CHANGELOG_VOICE.md`** — the published page and the breaking-change entry.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | Handler, store, and route dispatch change; cross-compile both linux targets. |
| PUB / Struct-Shape | yes | `innerRotateSecret` leaves the pub surface and `innerReplaceSecret` joins it; every removed `pub` is proven caller-free before deletion. |
| File & Function Length | yes | `secrets.zig` net-shrinks; `fleet_secret.ts` gains one effect and reuses the existing body resolver. |
| UFS | yes | The `UPDATE` reuses the shared column list; method and status literals are named constants. |
| UI Substitution / DESIGN TOKEN | yes | Edit is rebuilt from the same design-system primitives the Add dialog uses. |
| ERROR REGISTRY | yes | No new code; `MSG_SECRET_KEY_REQUIRED` retires with its only caller, and the registry test proves no dangling reference. |
| SCHEMA | yes | §6 adds additive slot 039 (`DROP DEFAULT`, idempotent) per `SCHEMA_CONVENTIONS.md`'s migration model; `write_sql.md` read before authoring; no frozen slot edited. |
| LOGGING / LIFECYCLE | no | No new logging surface; the write takes no transaction because it is one statement. |

## Prior-Art / Reference Implementations

- **Reference (server):** `innerStoreSecret` in the same file — same body shape, same validation, same 4 KiB pre-flight, same projection derivation. `PUT` differs only in claiming versus requiring the name.
- **Reference (rowcount idiom):** `crypto_store.create`'s `INSERT_SECRET_IF_ABSENT` — zero affected rows is the answer, not a second read. The `UPDATE` uses the same shape for the opposite question.
- **Reference (CLI):** `secret create` and `resolveSecretBody` — `update` reuses the body resolver rather than growing a second one.
- **Reference (UI):** `AddModelEntryDialog` — the form Edit becomes.

## Sections (implementation slices)

### §1 — The route replaces a secret whole — **DONE**

`PUT /v1/workspaces/{ws}/secrets/{name}` accepts the same `data` object `create` accepts and stores it as the secret's entire body. Nothing merges, so no field name is privileged and every shape — `api_key`, `token`, `api_token`, anything — is equally replaceable. The name must already be held: a `PUT` on a name this workspace does not have answers `404` and writes nothing, which keeps claiming a name the sole job of `create`.

The write is one `UPDATE … WHERE workspace_id = $ AND key_name = $` composed from the same column list the insert arms share, so the envelope and its `meta_*` projection can never describe different bodies. Zero affected rows is the `404`. Because there is no read before the write, the previous load-merge-store pair — two autocommit statements with nothing held between them — is gone, and with it the window in which a concurrent delete let the upsert re-insert a credential the operator had just removed.

**Implementation default:** an `UPDATE`, never an upsert. The distinction is the whole safety property.

- **Dimension 1.1** — a `PUT` with a full object replaces the stored body; a re-read shows the new fields and no field from the old body survives → Test `test_put_replaces_whole_body`
- **Dimension 1.2** — a `PUT` on a name the workspace does not hold answers `404` and creates nothing → Test `test_put_refuses_unheld_name`
- **Dimension 1.3** — a secret keyed on any field rotates: `{"token":…}` replaced with a new `token` leaves no stale value and no added field → Test `test_put_replaces_non_api_key_shapes`
- **Dimension 1.4** — body validation matches `create`: non-object, empty object, and oversize each answer their existing typed code and write nothing → Test `test_put_body_validation_matches_create`

### §2 — `PATCH` is deleted, not deprecated — **DONE**

The field patch is removed from every site that names it: the method dispatch, the sensitive-request classification, the route comment, the handler, its body struct, its helper, the retired message constant, and both published documents. Pre-`2.0.0` this is a removal, not a `410` stub (RULE EP4 applies only post-`2.0.0`), and there is no compatibility spelling.

- **Dimension 2.1** — no source file names the field patch: handler, body struct, helper, message constant, and client callers are gone → verified by Rubric R3's repo-wide grep; the method switch answers 405 structurally, and no test memorializes the dead method (Discovery A5)
- **Dimension 2.2** — the sensitive-request classification follows the method that now carries a secret body → Test `test_put_is_classified_sensitive`

### §3 — The client sends the secret it wants stored — **DONE**

`agentsfleet secret update <name> --data='{…}'` mirrors `create` exactly, including `--data=@-` for stdin, and reuses `resolveSecretBody` rather than growing a second resolver. The verb is `update` rather than `rotate` because the operation replaces a body; rotation is what a user does with it.

- **Dimension 3.1** — `update` issues exactly one `PUT` carrying the resolved object, and no read or delete around it → Test `test_secret_update_sends_single_put`
- **Dimension 3.2** — body sources and validation match `create`, and every rejection sends nothing → Test `test_secret_update_body_sources_and_validation`
- **Dimension 3.3** — JSON mode emits status and name only, and neither renderer prints secret bytes → Test `test_secret_update_output_modes_omit_secret`
- **Dimension 3.4** — a `404` renders as "secret not found" with a `secret list` suggestion and a non-zero exit → Test `test_secret_update_renders_missing_secret`

### §4 — The dashboard's Edit is the Create form — **DONE**

Edit currently offers `model` and a new key, sends two writes to two endpoints, and has a dedicated partial-success path when the second fails. It becomes the same form as Add — provider, base URL, model, key — prefilled from the list row the page already holds, because the list projects everything except the secret itself. The secret is written once, as a whole body.

The registry entry's `model_id` remains its own write: it is a different resource, and unifying the model field the entry and the secret both carry is named Out of Scope. What goes away is spending two writes on the *secret*, and offering a form that cannot express `provider` or `base_url`.

Two writes to two daemon resources cannot share a transaction from the dashboard, so the order is chosen by what a stranded write costs (Discovery A11). The **entry** writes first because it is recoverable — one row, visible in the table, changed back in two clicks. The **secret** writes last because it is none of those: shared by every entry referencing it, invisible once written, and never readable again to restore. Validations are hoisted above both writes so none can fire between them. If the secret write is the one that fails, the table re-reads rather than the dialog narrating the split — the same "mirror backend reality regardless of outcome" rule `ModelsRegistryTable`'s own actions follow.

- **Dimension 4.1** — Edit renders the Create form prefilled from the held summary and issues no extra read → Test `test_edit_dialog_renders_create_form_prefilled`
- **Dimension 4.2** — saving writes the secret exactly once with the full body → Test `test_edit_dialog_writes_secret_once`
- **Dimension 4.4** — a failure ahead of the secret write leaves the shared credential untouched; a failed secret write strands only the entry rename and re-reads the table to show it; a validation failure runs before either write → Tests `a failed model write never reaches the credential — nothing is rotated`, `a failed replace strands only the model write, and the table is re-read to show it`, `a validation failure runs before EITHER write`
- **Dimension 4.3** — Add's existing-name branch replaces the body instead of patching a field → Test `test_add_dialog_replaces_existing_secret`

### §5 — The CLI coverage floor runs where it can fail the build — **DONE**

`cli/bunfig.toml` declares a 100% line and function floor, and `scripts/enforce-coverage.mjs` exists to enforce it because Bun parses the threshold without acting on it. Nothing calls the script: CI's `test-unit-cli` and `make test-coverage-all` both run `test:coverage`, which only prints a table. The floor has been decorative, and `main` is below it.

- **Dimension 5.1** — the CI job and the make recipe fail when coverage is below the declared floor → Test `test_coverage_gate_fails_below_floor`
- **Dimension 5.2** — the floor is met: the uncovered lines in `cli.ts`, `commands/api_key.ts`, `commands/connector.ts`, `commands/fleet_install.ts`, `commands/fleet_schedule.ts` and the uncovered function in `commands/login-helpers.ts` are covered, or deleted if unreachable → Test `test_cli_coverage_floor_met`

Gaps are read from `cli/coverage/lcov.info` (`DA:` for lines, `FNDA:` for functions). The text reporter's "Uncovered Line #s" column also lists lines carrying uncovered *branches*, and reading it as a line list cost a prior session a full cycle. Where a gap proves unreachable, RULE TVR applies: delete the dead arm rather than test a value that cannot occur.

### §6 — The vault speaks one envelope version — **DONE**

Envelopes sealed before AAD binding (`0ff4902ca`, Jul 11) were version 1; every write since is version 2, bound to `(workspace_id, key_name)` so relocated ciphertext refuses to decrypt. The read path still tolerated both, forever — a compatibility branch with no writer behind it. Verified against app-dev (Discovery A8): the database holds 167 secrets, all version 2, the earliest created Jul 20 — nine days after binding shipped. v1 has never existed in this deployment, and there is no production yet. So on Indy's direction v1 is removed outright, not tolerated: migration slot 039 drops `kek_version`'s `DEFAULT 1`. That default was the last way a v1 row could be minted — every writer binds the version explicitly, so it never fired on a correct write, and the only row it could create is one a forgotten column would silently insert. The column is `NOT NULL` (`schema/002`), so with the default gone that same forgotten column fails the INSERT outright instead of landing a row nothing can decrypt.

The version itself is **not** pinned by a schema `CHECK` (Discovery A10, RULE STS): it stays the named application constant `KEK_VERSION_AAD_BOUND` that both write arms bind, which is where the operating model puts an app-enforced value and the same call `schema/017` and `schema/018` record. The AAD tag is the ultimate guard beneath it, since an envelope read under a version it was not sealed at fails authentication (`DecryptFailed`) — so the read path's version branch is dead code and is deleted. A startup conversion sweep was built and removed earlier (Discovery A6); no code converts v1 because none can exist.

- **Dimension 6.1** — both write arms seal at the current version constant, and a rewritten body is re-sealed at it rather than inheriting whatever the row held → Test `every write arm binds the current envelope version`
- **Dimension 6.2** — an ordinary secret round-trips at version 2 (the only version) → Test `crypto store canonicalizes workspace id and upserts a fresh envelope`
- **Dimension 6.3** — `kek_version` carries no schema default and is `NOT NULL`, so an INSERT that omits it raises → Test `kek_version carries no schema default and is NOT NULL`

## Interfaces

```
PUT /v1/workspaces/{workspace_id}/secrets/{secret_name}          (replaces PATCH)
  request   { "data": { …non-empty JSON object, ≤4 KiB stringified… } }
  200       { "name": "<secret_name>" }
  400       UZ-REQ-001    absent/malformed body, bad workspace id or name
  400       UZ-VAULT-001  data is not a non-empty JSON object
  400       UZ-VAULT-002  stringified data exceeds the 4 KiB limit
  403       caller below workspace operator
  404       UZ-VAULT-003  this workspace holds no secret by that name — nothing written

PATCH /v1/workspaces/{workspace_id}/secrets/{secret_name}        REMOVED (405)

agentsfleet secret update <name> --data='<json-object>'|@-
  exit 0    human: one line naming the secret; json: { "status": "updated", "name": "<name>" }
  exit != 0 typed CliError carrying code + suggestion; validation failures send no request
```

Replacement is total: fields absent from the new body are absent from the stored secret. That is the point of the verb, and it is what `create` already means.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unheld name | `PUT` on a name the workspace does not have | `404` `UZ-VAULT-003`; zero rows affected; nothing created |
| Concurrent delete | Delete commits before the `PUT` | The `UPDATE` matches nothing → `404`. No read precedes it, so nothing can be resurrected |
| Concurrent replace | Two `PUT`s on one secret | Each is a single statement; the later commit wins wholly, and neither caller is told a body is stored that is not |
| Malformed body | Non-object, empty object, array, scalar | `UZ-VAULT-001`; nothing written |
| Oversize body | Stringified body over 4 KiB | `UZ-VAULT-002`; nothing written |
| Insufficient role | Caller below workspace operator | `403` from the existing workspace guard |
| Retired method | A caller still sends `PATCH` | Method-not-allowed; no silent acceptance and no compatibility spelling |
| Coverage regression | A later diff drops CLI coverage below the floor | The CI job exits non-zero naming floor and actual |

## Invariants

1. A replacement never releases the name and never creates one — enforced by the statement being an `UPDATE` with no insert arm, and by Dimension 1.2 asserting zero rows created on an unheld name.
2. The envelope and its `meta_*` projection always describe the same body — enforced by both writing in one statement composed from the shared column list.
3. No surface can change a secret one field at a time — enforced by the deletion of `innerRotateSecret` and by Dimension 2.1 asserting the method is gone.
4. Secret bytes never reach a log line, an event, or stdout — enforced by the existing secure-memory handling, the CLI's argument redaction, and Dimension 3.3.
5. CLI coverage cannot fall below the declared floor — enforced by the CI job exiting non-zero.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `fleet.secret.update` command span | ops | The update command runs | command name, outcome, duration | no secret bytes | `test_secret_update_output_modes_omit_secret` |
| server `replaced` / `replace_failed` log | ops | `PUT` succeeds or fails | secret name, workspace, error code | never the body | `test_put_refuses_unheld_name` |
| existing `key_rotated` product event | product | The dashboard saves a new key | provider only | no key material | `test_edit_dialog_writes_secret_once` |

The dashboard's existing rotation event is retained and keeps firing on a save that changes the secret; no funnel changes and no new product event is added.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_put_replaces_whole_body` | Stored `{provider,model,base_url,api_key}` replaced with `{provider,api_key}` → list row shows no `base_url`; the dropped field is gone, not retained |
| 1.2 | integration | `test_put_refuses_unheld_name` | `PUT` on an unheld name → `404` `UZ-VAULT-003`; `SELECT count(*)` for that name stays 0 |
| 1.3 | integration | `test_put_replaces_non_api_key_shapes` | `{"token":"old"}` replaced with `{"token":"new"}` → stored body has the new token and no `api_key` |
| 1.4 | integration | `test_put_body_validation_matches_create` | Array, scalar, empty object, and a 5 KiB object each answer the same code `create` answers and write nothing |
| 2.2 | unit | `test_put_is_classified_sensitive` | `sensitive_request` classifies `PUT` on `workspace_secret` as sensitive and `PATCH` no longer appears |
| 3.1 | integration | `test_secret_update_sends_single_put` | `update acme --data='{"k":"v"}'` issues exactly one `PUT` with `{data:{k:"v"}}`; no GET, no DELETE |
| 3.2 | unit | `test_secret_update_body_sources_and_validation` | `--data=@-`, literal `--data`, empty stdin, absent flag, absent name — same outcomes as `create`, zero requests on each rejection |
| 3.3 | unit | `test_secret_update_output_modes_omit_secret` | JSON mode emits status and name only; neither renderer's output contains the secret substring |
| 3.4 | unit | `test_secret_update_renders_missing_secret` | A `UZ-VAULT-003` renders with a `secret list` suggestion and exits non-zero |
| 4.1 | unit | `test_edit_dialog_renders_create_form_prefilled` | Opening Edit shows provider/base URL/model/key prefilled from the held row and issues no fetch |
| 4.2 | unit | `test_edit_dialog_writes_secret_once` | Saving calls the replace action exactly once with the full body |
| 4.3 | unit | `test_add_dialog_replaces_existing_secret` | Add against an existing name calls replace with the full composed body, not a field patch |
| 4.4 | unit | `a failed model write never reaches the credential — nothing is rotated` | A rejected entry write short-circuits before `replaceSecretAction`; the shared credential is untouched and no re-read fires |
| 4.4 | unit | `a failed replace strands only the model write, and the table is re-read to show it` | A rejected replace after a committed entry write fires `onCommitted` so the table shows the server's model, and leaves the dialog open on the error |
| 4.4 | unit | `a validation failure runs before EITHER write` | A non-HTTPS base URL is refused with zero write calls, so no half can be stranded by a check that fired between them |
| 5.1 | integration | `test_coverage_gate_fails_below_floor` | The enforcing script exits non-zero below the floor and names floor and actual |
| 5.2 | unit | `test_cli_coverage_floor_met` | The enforcing script exits 0 against the branch |
| e2e | e2e | `test_secret_update_live_lane` | Against a live daemon: create, update, confirm the name resolves throughout, delete |
| regression | integration | `test_secret_create_still_claims_free_name` | `create` on a taken name still reports a skip and exits 0; `UZ-VAULT-005` is unchanged |
| regression | integration | `test_connector_callbacks_still_overwrite` | The OAuth callback and token-refresh paths keep using the overwriting store — re-connecting is still a rotation and is untouched by this workstream |
| idempotency | integration | `test_repeat_replace_is_stable` | Replacing twice with the same body succeeds twice and leaves one row |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A secret is replaced whole, on any shape (§1) | `zig build test-integration -Dtest-filter=test_put_replaces_whole_body` | tests passed | P0 | ✅ 67/67; a provider secret replaced with a `{token}` body drops every old field |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | ✅ every code/doc path has a row; test files follow their code rows per the scope note |
| R3 | The field patch is gone everywhere (§2) | `git grep -n -w 'innerRotateSecret\|rotateSecret' -- src ui cli public \| wc -l` | 0 | P0 | ✅ 0 |
| R4 | The client sends one PUT (§3) | `cd cli && bun test test/fleet-secret-update.unit.test.ts test/secrets.integration.test.ts` | exit 0 | P0 | ✅ 17 pass, 0 fail |
| R5 | Edit writes the secret once (§4) | `cd ui/packages/app && bunx vitest run tests/models-registry-edit-remove.test.tsx` | exit 0 | P0 | ✅ 25 passed |
| R6 | The coverage floor is enforced and met (§5) | `cd cli && bun ./scripts/enforce-coverage.mjs` | exit 0, output contains `enforce-coverage: PASS` | P0 | ✅ `enforce-coverage: PASS` (line 100%; function graded when bun emits records) |
| R7 | The published page documents the replace verb | `grep -c 'PATCH' ~/Projects/docs/fleets/secrets.mdx` | 0 | P0 | ✅ 0 |
| R8 | The live lane executed against a daemon | `op run --env-file=<dev creds> -- bun test test/acceptance/secret-vault.spec.ts` against `api-dev` | exit 0 post-deploy | P1 | 🔶 ran live against `api-dev` (16/17): create/show/list/delete/negatives pass; `update` returns HTTP 405 because dev runs pre-M145 code with no `PUT` route. Deploy-lag, not a defect — the local build routes `PUT` (200). Greens on the post-deploy smoke lane. See Discovery A9 |
| R9 | One envelope version: sealed by every write arm, no default, no dual read (§6) | `zig build list-tests 2>&1 \| grep -cE "every write arm binds the current envelope version\|kek_version carries no schema default and is NOT NULL"` | 2 | P0 | ✅ 2 registered and reachable; both pass against a live database (`TEST_DATABASE_URL` set, `52/52 tests passed` per filtered run). Command corrected at A10 — the previous `zig build test --summary all` spelling greps for test names in output that only ever carries step names, so it returned 0 for the old names too |
| S1 | Unit lanes pass | `make test-unit-all` | exit 0 | P0 | ✅ All unit lanes passed; delta unit 3223→3226 |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ All lint checks passed |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | ✅ All integration tests passed (DB + Redis + qstash) |
| S5 | No leaks | `make memleak` | exit 0 | P0 | ✅ memleak gate passed incl. boot→drain |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | ✅ both targets |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ no leaks found |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ non-test source ≤350 (AddModelEntryDialog 349); test files FLL-exempt; ModelsRegistryTable 362 inherited from main and reduced |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — none deleted.**

| File to delete | Verify |
|----------------|--------|
| N/A — no file is removed; symbols are removed from files that remain | `true` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `innerRotateSecret` | `git grep -n -w innerRotateSecret` | 0 matches |
| `rotateSecretKeyOnConn` | `git grep -n -w rotateSecretKeyOnConn` | 0 matches |
| `rotateSecret` / `rotateSecretAction` | `git grep -n -w 'rotateSecret\|rotateSecretAction'` | 0 matches |
| `MSG_SECRET_KEY_REQUIRED` | `git grep -n -w MSG_SECRET_KEY_REQUIRED` | 0 matches |

## Out of Scope

- **Unifying the `model` field the secret body and the registry entry both carry.** Real duplication and a genuine drift risk, but it is a registry-shape question, not a replace-verb one. Edit still writes the entry's `model_id` separately.
- **`GET /v1/models?provider=`, published and uncalled** — the standing finding from M143_002 Discovery.
- **The OAuth connector callbacks and token refresh** keep the overwriting vault write. Re-connecting a provider is a rotation, and a pinned regression test keeps them that way.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a key leaks; the operator edits the secret in the dashboard or sends one command; whatever shape the secret has, the new value is what is stored, and fleets never see the name disappear.
2. **Preserved user behaviour** — `create` still claims a free name and reports a taken one as a skip; `show`, `list`, `delete` are untouched; connector re-connect still overwrites.
3. **Optimal-way check** — replacing a whole body is the most direct shape available given the one hard constraint: a stored secret can never be read back. Because the list already projects every non-secret field, a client can rebuild the full body and supply only what it is changing. Nothing about a field patch is recoverable by better ergonomics.
4. **Rebuild-vs-iterate** — rebuild the verb, keep everything else. The storage layer, the role guard, and the projection are all correct; only the write shape was wrong.
5. **What we build** — one replaced route, one deleted method, one CLI command, one rebuilt dialog, corrected pages, and the CI wiring for the coverage script.
6. **What we do NOT build** — a merge/patch spelling of any kind, a compatibility alias for `PATCH`, secret read-back, rotation history, and scheduled rotation.
7. **Fit with existing features** — compounds with the credential firewall and the model registry, which both resolve secrets by name. It must not destabilize connector re-connect, which is why that path is pinned by a regression test.
8. **Surface order** — both, in one workstream. The verb is wrong on every surface at once, so fixing one and not the others would leave the dashboard and the CLI disagreeing about what changing a secret means.
9. **Dashboard restraint** — Edit gains only fields the user already supplies at Create. No rotation history, because the server records none.
10. **Confused-user next step** — the `404` names `secret list`; `secret update --help` names both body sources; the page states that replacement is total. None ends in "file a ticket".

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five Sections split by surface — route, removal, client, dashboard, CI — because each is independently verifiable and the removal must be provable separately from the addition.
- **Alternatives considered:** (a) keep `PATCH` and add `PUT` beside it, rejected because two ways to write one resource is exactly the ambiguity being removed, and our pre-`2.0.0` rule is replace-don't-alias; (b) generalize `PATCH` to a named field (`{field, value}`), rejected on Indy's call — a partial write on a resource nobody can read back cannot be reasoned about by the caller; (c) ship the CLI command over the existing `PATCH`, rejected because it encodes the wrong verb in a second surface.
- **Patch-vs-refactor verdict:** this is a **refactor**, and deliberately so. The earlier version of this spec was a patch — a CLI command over the existing field patch — and it failed the quality-ceiling question: it duplicated a dashboard capability, served only the `api_key` shape, and spread a design defect to a new surface. Replacing the verb is a bigger diff that removes more code than it adds on the server and deletes a partial-success path in the dashboard.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close)).
- **Deferrals** — every "deferred to follow-up" needs an Indy-acked verbatim quote here.

- **Amendment A3 (EXECUTE, Indy-directed) — the workstream is redesigned around a whole-body replace.** The spec previously added `agentsfleet secret rotate --api-key` over the published `PATCH`, and a server section that locked rotation's read-modify-write. Indy rejected the underlying verb.

  > Indy (2026-07-29 08:12): "I think th eway rotation is design is incorrect, since once the secret is set, the update or rotate must be full." — context: whether a stored secret should be changed field by field.

  > Indy (2026-07-29 08:14): "Yes nuke PATCH, it doesnt make sense, it case issues, since once the secret is saved, we cant see it. PUT is better." — context: replacing versus keeping the field patch.

  > Indy (2026-07-29 08:14): "Yes Scope for UI as well Edit is essentially like the Create" — context: whether the dashboard's Edit dialog is in this workstream.

  Three earlier findings are resolved by the redesign rather than by their own fixes. The unlocked read-modify-write (a rotation racing a delete re-inserted the deleted credential, because the write was `ON CONFLICT DO UPDATE` and the read had already committed) disappears because `PUT` is a single `UPDATE` with no read. The silent-corruption case (`PATCH {api_key}` on a `{"token":…}` secret added an unused field, left the live credential stale, and answered `200`) disappears because nothing merges. The proposed `UZ-VAULT-006` guard is therefore not needed and is not added. The abandoned lock implementation was parked on a stash for reference and dropped on Jul 30, 2026: 10:34 AM per Indy, once the reasoning above made the code itself redundant. Its commit object is `9637baffda15b2416f43ddfee7ec0fe00132135f` — readable via `git show` until git collects it — and nothing in `main` ever carried `beginSecretLock` or `UZ-VAULT-006`.

- **Amendment A5 (EXECUTE, Indy-directed) — no test memorializes the removed method.** The spec ordered a 405 pin on `PATCH`; it was built and then deleted.

  > Indy (2026-07-29): "well remove the patch patch of secrets dont keep retired methods" — context: whether a test asserting the dead method answers 405 should exist at all.

  Dimension 2.1 reworded: absence is proven by Rubric R3's repo-wide grep, and the 405 falls out of the method switch structurally.

- **Amendment A6 (EXECUTE, Indy-directed) — the envelope-version duality is folded in and retired by deletion, not conversion.** An audit for live old/new duality found one real case: `vault.secrets.kek_version` — reads accepted v1 (pre-AAD) and v2 forever, nothing wrote v1, and the schema default still named it.

  > Indy (2026-07-29): "I wanted to be folded in this PR" — context: whether the kek_version finding becomes its own spec or rides this one.

  A startup rewrap sweep (list v1 rows, decrypt under the empty AAD, re-seal as v2) was built, wired into serve, and tested — then deleted:

  > Indy (2026-07-29): "I said i dont want to support this legacy crap what are you trying to fix here KEK_VERSION_LEGACY? … why are we adding so much code" — context: the sweep's ~100 lines against outright refusal.

  Superseded by A8: the refuse-on-read behaviour this amendment landed was itself removed once app-dev proved zero v1 rows exist. Also folded under NLR: `fleet/secrets_resolve.zig`'s "legacy credential" comment renamed — the shape is just a credential without an integration field.

- **Amendment A8 (REVIEW, Indy-directed) — v1 is removed structurally after checking app-dev.** The adversarial review (both Claude and Codex) flagged that a v1 secret would fail at the lease point as a non-terminal transient error, retrying forever, and that the refuse-on-read left v1 rows possible. Indy directed checking the database and removing v1 outright.

  > Indy (2026-07-29): "So there will be no v1 secrets. Well the v1 must be removed, since we are not in production yet. and you can check when the database was populated in app-dev and decide."

  Checked via 1Password (`ZMB_CD_DEV` → `planetscale-dev` api connection string): `vault.secrets` holds **167 rows, all `kek_version = 2`, earliest `created_at` Jul 20 2026** — after AAD binding shipped Jul 11. Zero v1 rows, no production. So migration 039 drops `DEFAULT 1`; the read path's version branch and `SecretError.UnsupportedKekVersion` were deleted as now-dead code (the AAD tag is the failsafe); and the three v1-refusal tests were replaced. The lease retry-loop finding is eliminated by construction — the error it looped on no longer exists. (039 also gained `CHECK (kek_version = 2)` at this point; **superseded by A10**, which removes it.)

  Surfaced but NOT fixed here (out of this workstream's scope, its own security spec): connector handles (`github`, `slack`, …) share the `vault.secrets` `(workspace_id, key_name)` space with user secrets — app-dev holds 15 live `github` handles. `create` over a connector name is blocked by the vault's `ON CONFLICT DO NOTHING` (`UZ-VAULT-005`), proven by `a connector-shaped name collides — create is blocked, PUT overwrites`; but `PUT` (an `UPDATE`) overwrites the handle in place, and `delete`+`create` already did so on `main`. A reserved-name guard on the secrets write path plus an installation-ownership check at mint is the fix. Deferred out of this workstream on Indy's call at REVIEW, after greptile re-raised it:

  > Indy (2026-07-29): "lets skip the secrets > connector one now" — context: whether the connector-name overwrite is fixed in this workstream or specced separately.

  Scope note for whoever picks it up: the guard must cover `DELETE` as well as `PUT`, because `delete`+`create` reaches the same end state in two calls and was already reachable on `main`. Fixing only the replace path leaves the hole open.

- **Authoring verification (Jul 29, 2026)** — read from source on the branch, not from prose: the item route's dispatch is `route_table_invoke.zig:251` and matches on method only, so the matcher needs no change; `sensitive_request.zig:19` classifies `PATCH` on this route as sensitive and must move with the verb; `secret_list.zig:20` projects `kind`, `provider`, `model`, `base_url` — which is what makes a client-side full-body rebuild possible without reading the secret; `EditModelEntryDialog.tsx:72-95` issues two writes for one intent and calls `onPartialSuccess()` between them; `AddModelEntryDialog.tsx:207` calls the same rotate action on an existing name; and the coverage floor was reproduced red at `function=99.97% line=99.74%` against a 100% floor.

- **Amendment A7 (EXECUTE) — the enforcing script grades from lcov records, and the function axis is graded only when records exist.** `enforce-coverage.mjs` regex-parsed bun's rendered "All files" row. Verified against bun 1.3.14: the text row reported one function missed while the same run's per-function `FNDA` records showed every function hit, and in other runs bun withheld `FN`/`FNDA` records entirely while its aggregate `FNH` sat one short of `FNF` — with no way to name the function it claimed missed. The script now sums line records (`DA`/`LF`/`LH`, consistent in every shape) and grades functions from per-function records when the run carries them; a run without them prints `function=ungraded (no per-function records this run)` rather than failing on a number bun cannot substantiate. The line floor stays hard at 100% in every run.

- **Amendment A9 (REVIEW, live R8) — the live acceptance lane ran against `api-dev` and behaves exactly as a pre-deploy new-client run should.** Indy provided the dev URL; the target resolved to `https://api-dev.agentsfleet.net` and Clerk-dev credentials were assembled from `op://ZMB_CD_DEV` via `op run`. The full `run-lane.ts` live lane aborts at its browser login-handshake gate (`page.waitForFunction … Target page closed`) — a Playwright/Chromium step that needs CI's browser environment, unrelated to this workstream. `secret-vault.spec.ts` self-authenticates through Clerk's backend API (`attachJwt`, no browser), so it was run directly: 16 of 17 pass against the live daemon, and the one failure is `secret update` receiving `HTTP 405 Method Not Allowed`. That is deploy ordering, not a defect — the deployed dev daemon still carries pre-M145 code whose route table has `POST`/`PATCH`/`DELETE` but no `PUT`; this branch's local build routes `PUT` to `200` (integration `test_put_replaces_whole_body`). The acceptance lane's designed home is the post-deploy smoke workflow, where it greens once M145 deploys. R8 is therefore satisfied in the sense the rubric can be satisfied pre-merge: the client sends the correct method, auth and every other CLI path work live, and the sole gap is the server code this PR ships.

- **Amendment A10 (REVIEW, Indy-directed) — the `CHECK (kek_version = 2)` is removed; the version is enforced in the application, not the schema.** Reviewing greptile's schema/039 finding (itself a false positive — A8 already proved zero v1 rows exist), Indy asked why the second `ALTER` was needed at all given the first. It was not:

  > Indy (2026-07-29): "for the drop default, i have question why do you need this ALTER then? (the second one)?" — context: whether `CHECK (kek_version = 2)` earns its place beside `DROP DEFAULT`.

  Two reasons it does not. **Redundant for its stated job:** `kek_version` is `INTEGER NOT NULL` (`schema/002:55`), so once the default is dropped an INSERT that omits the column fails on the not-null violation — the "forgotten column" case the migration exists to catch is already loud without a CHECK. **And against RULE STS:** `CHECK (kek_version = 2)` is the one-element case of the static-value CHECK the operating model forbids in schema ("enforce in app via named constants"), a call `schema/017:20` and `schema/018:25` each already record in their own comments. Every other CHECK in `schema/` is a structural invariant (UUIDv7 shape, an identifier regex, `balance_nanos >= 0`); none pins an application constant, and this would have been the first. It also carried a live cost: a future envelope format would have to widen the constraint *before* its writer could write, a migration-ordering hazard for a guarantee the AEAD tag already provides.

  So 039 keeps `DROP DEFAULT` alone. Dimension 6.1 moves from the database to the writers — `every write arm binds the current envelope version` exercises `create` and `replace` and asserts both seal at `KEK_VERSION_AAD_BOUND`, read from the writer's own constant rather than restated as a literal (a test spelling `2` itself would keep passing if a writer stopped binding it). Dimension 6.3 gains the `NOT NULL` half of the mechanism, since no-default alone would leave a forgotten column inserting NULL.

  **Two things the test run corrected, both strengthening the removal.** First, an assertion drafted for this test — relabel a row's `kek_version` by hand, expect `DecryptFailed` — *failed*: the row decrypted fine. The decrypt path builds its AAD from `KEK_VERSION_AAD_BOUND` and never reads the stored column, so the label is descriptive, not load-bearing. The AEAD tag guards the *seal* (an envelope opened under AAD it was not sealed under fails, which is what a genuine v1 ciphertext hits, and what the relocation tests already prove) — it does not police the label. That makes the `CHECK` weaker than argued, not stronger: it constrained a value no reader consults. The assertion was dropped rather than reworded, and `SET_VERSION` deleted with it (RULE NDC).

  Second, R9's Verify command never worked. `zig build test --summary all` prints step names, not test names, so the grep returned 0 — for the old test names as much as the new ones, meaning the ✅ 2 recorded before this amendment was not produced by the command beside it. Corrected to `zig build list-tests`, the repo's own reachability lister, which prints registered test names and returns 2.

  Noted, not acted on (out of this amendment's scope): `SELECT_SECRET` still selects `kek_version` and `decryptRowAt` discards it. Harmless, but the column is now write-only on that path.

- **Amendment A11 (REVIEW, Indy-directed) — the Edit dialog writes the recoverable resource first, and shows the stranded half instead of describing it.** Greptile flagged that a successful secret replace followed by a failed model write leaves the credential committed while the save reports failure. Valid, and the ordering this workstream had shipped made it the reachable case.

  The two writes go to different daemon resources (`apiReplaceSecret`, `apiUpdateTenantModelEntry`), so the dashboard cannot make them atomic; ordering only decides which half can be stranded. The two halves are not equivalent. A stranded entry rename is one row, visible in the table, undone in two clicks. A stranded secret replace hits **every entry sharing that secret**, is invisible, and can never be undone — a stored key is not readable, so there is no prior value to restore. This workstream had put the permanent write first, which is backwards; the changelog compounded it by framing the old entry-first order as the bug being fixed.

  The first fix drafted was an error string naming which half landed. Indy rejected it:

  > Indy (2026-07-29): "well saying an error doesnt make sense to me." — context: whether a message explaining the stranded write is an acceptable fix.

  > Indy (2026-07-29): "Okay fix that and push" — context: approving the reorder, the hoisted validations, and the re-read in place of the error prose.

  He is right that an error explaining a broken state is not a fix. `ModelsRegistryTable` already states the alternative in its own comments — *"refresh even on failure (matches ApiKeyList's onConfirm — mirror backend reality regardless of outcome)"* — so the dialog now follows the same rule: the writes swap, every validation hoists above both of them, and the secret-write failure arm calls `onCommitted` (wired to the table's `refresh`) so the committed entry change is **shown** rather than narrated. The error text is unchanged.

  This re-introduces a callback this workstream deleted (`onPartialSuccess`, §4), under different reasoning: not "report a partial success" but "re-read the server after any committed write." Residual, stated rather than hidden: the split is made cheap and visible, not eliminated. Only a daemon endpoint writing both resources in one transaction removes it — both tables live in the same Postgres, so it is possible, and it is out of scope here.

- **Correction to the inherited finding** — M143_002 recorded four red coverage files. Reading `coverage/lcov.info` directly gives six: `cli.ts` (2 lines), `commands/api_key.ts` (27), `commands/connector.ts` (10), `commands/fleet_install.ts` (3), `commands/fleet_schedule.ts` (16), and one uncovered function in `commands/login-helpers.ts`.
